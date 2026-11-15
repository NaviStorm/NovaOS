//! Politique du gestionnaire de services en espace utilisateur. Le catalogue
//! installé est la seule source des images, arguments et droits d'un lancement.
//! Un numéro d'essai corrèle les événements du gestionnaire. L'adaptateur
//! conserve séparément le droit d'observation de l'incarnation correspondante.

pub const VERSION: u8 = 1;
pub const MAX_SERVICES: usize = 12;
pub const MAX_ARGS: usize = 4;
pub const MAX_RETRIES: usize = 8;
const LOG_SIZE: usize = 16;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Restart {
    Never,
    Failure,
    Always,
}

#[derive(Copy, Clone, Debug)]
pub struct Declaration {
    pub version: u8,
    pub id: u64,
    pub image: u64,
    pub args: &'static [u64],
    pub required: &'static [u64],
    pub optional: &'static [u64],
    pub caps: u64,
    pub auto: bool,
    pub restart: Restart,
    pub start_ticks: u64,
    pub stop_ticks: u64,
    pub heartbeat_ticks: u64,
    pub retries: u8,
    pub window: u64,
    pub backoff: u64,
    pub log: bool,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum State {
    Registered,
    Waiting,
    Starting,
    Ready,
    Running,
    Stopping,
    Stopped,
    Failed,
    Restarting,
}

impl State {
    fn live(self) -> bool {
        matches!(
            self,
            Self::Starting | Self::Ready | Self::Running | Self::Stopping
        )
    }

    fn available(self) -> bool {
        matches!(self, Self::Ready | Self::Running)
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Cause {
    Registered,
    Requested,
    Started,
    Available,
    Exited,
    Crash,
    Killed,
    Communication,
    Capability,
    StartTimeout,
    StopTimeout,
    Dependency,
    Restart,
    Limit,
    LaunchError,
    Invalid,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Error {
    InvalidDeclaration,
    Duplicate,
    MissingDependency,
    Cycle,
    Unknown,
    Unregistered,
    InvalidState,
    Stale,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Action {
    Launch { id: u64, attempt: u64 },
    Stop { id: u64, attempt: u64 },
    Kill { id: u64, attempt: u64 },
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct Event {
    pub id: u64,
    pub attempt: u64,
    pub from: State,
    pub to: State,
    pub cause: Cause,
    pub exit: u64,
    pub dependency: u64,
}

#[derive(Copy, Clone)]
struct Slot {
    registered: bool,
    wanted: bool,
    state: State,
    attempt: u64,
    deadline: u64,
    stop: Option<Cause>,
    dependency: u64,
    killed: bool,
    // Au plus huit instants triés. Le budget est glissant même à sa frontière.
    restarts: [u64; MAX_RETRIES],
    used: usize,
}

impl Slot {
    const EMPTY: Self = Self {
        registered: false,
        wanted: false,
        state: State::Registered,
        attempt: 0,
        deadline: 0,
        stop: None,
        dependency: 0,
        killed: false,
        restarts: [0; MAX_RETRIES],
        used: 0,
    };
}

pub struct Policy<'a> {
    manifest: &'a [Declaration],
    implicit_starts: u16,
    slots: [Slot; MAX_SERVICES],
    required: [u16; MAX_SERVICES],
    optional: [u16; MAX_SERVICES],
    order: [usize; MAX_SERVICES],
    journal: [Option<Event>; LOG_SIZE],
    head: usize,
    count: usize,
    dropped: u64,
}

impl<'a> Policy<'a> {
    pub fn new(manifest: &'a [Declaration]) -> Result<Self, Error> {
        Self::build(manifest, false)
    }

    /// Catalogue administré. Au boot, une dépendance inhibée reste enregistrée,
    /// et le consommateur attend l'autorité de lancement ou la prochaine configuration.
    pub fn configured(manifest: &'a [Declaration]) -> Result<Self, Error> {
        Self::build(manifest, true)
    }

    fn build(manifest: &'a [Declaration], configured: bool) -> Result<Self, Error> {
        if manifest.len() > MAX_SERVICES {
            return Err(Error::InvalidDeclaration);
        }
        let mut policy = Self {
            manifest,
            implicit_starts: if configured { 0 } else { u16::MAX },
            slots: [Slot::EMPTY; MAX_SERVICES],
            required: [0; MAX_SERVICES],
            optional: [0; MAX_SERVICES],
            order: [0; MAX_SERVICES],
            journal: [None; LOG_SIZE],
            head: 0,
            count: 0,
            dropped: 0,
        };
        for (i, d) in manifest.iter().enumerate() {
            if d.version != VERSION
                || d.id == 0
                || d.image == 0
                || d.args.len() > MAX_ARGS
                || d.required.len() + d.optional.len() > MAX_SERVICES
                || d.start_ticks == 0
                || d.stop_ticks == 0
                || usize::from(d.retries) > MAX_RETRIES
                || (d.restart != Restart::Never && (d.window == 0 || d.backoff == 0))
            {
                return Err(Error::InvalidDeclaration);
            }
            if manifest[..i].iter().any(|other| other.id == d.id) {
                return Err(Error::Duplicate);
            }
            let mut dependencies = 0;
            for (k, ids) in [d.required, d.optional].iter().enumerate() {
                let mut mask = 0;
                for &id in *ids {
                    let j = policy.index(id).map_err(|_| Error::MissingDependency)?;
                    if dependencies & (1 << j) != 0 {
                        return Err(Error::Duplicate);
                    }
                    mask |= 1 << j;
                    dependencies |= 1 << j;
                }
                if k == 0 {
                    policy.required[i] = mask;
                } else {
                    policy.optional[i] = mask;
                }
            }
        }
        let mut seen = 0u16;
        for pos in 0..manifest.len() {
            let i = (0..manifest.len())
                .find(|&i| {
                    seen & (1 << i) == 0 && (policy.required[i] | policy.optional[i]) & !seen == 0
                })
                .ok_or(Error::Cycle)?;
            policy.order[pos] = i;
            seen |= 1 << i;
        }
        if configured {
            for (i, d) in manifest.iter().enumerate() {
                policy.register(d.id)?;
                if d.auto {
                    policy.implicit_starts |= 1 << i;
                    policy.slots[i].wanted = true;
                    policy.transition(i, State::Waiting, Cause::Requested, 0, 0);
                }
            }
        } else {
            for d in manifest.iter().filter(|d| d.auto) {
                policy.register(d.id)?;
                policy.start(d.id)?;
            }
        }
        Ok(policy)
    }

    pub fn blocking_provider(&self, id: u64) -> Option<u64> {
        let i = self.index(id).ok()?;
        if self.slots[i].state != State::Waiting {
            return None;
        }
        self.unavailable(i).map(|source| self.manifest[source].id)
    }

    fn index(&self, id: u64) -> Result<usize, Error> {
        self.manifest
            .iter()
            .position(|d| d.id == id)
            .ok_or(Error::Unknown)
    }

    fn registered_index(&self, id: u64) -> Result<usize, Error> {
        let i = self.index(id)?;
        self.slots[i]
            .registered
            .then_some(i)
            .ok_or(Error::Unregistered)
    }

    pub fn state(&self, id: u64) -> Result<State, Error> {
        Ok(self.slots[self.registered_index(id)?].state)
    }

    pub fn attempt(&self, id: u64) -> Result<u64, Error> {
        Ok(self.slots[self.registered_index(id)?].attempt)
    }

    pub fn dropped_events(&self) -> u64 {
        self.dropped
    }

    pub fn pop_event(&mut self) -> Option<Event> {
        if self.count == 0 {
            return None;
        }
        let value = self.journal[self.head].take();
        self.head = (self.head + 1) % LOG_SIZE;
        self.count -= 1;
        value
    }

    fn record(&mut self, i: usize, from: State, cause: Cause, exit: u64, dependency: u64) {
        let (attempt, to) = (self.slots[i].attempt, self.slots[i].state);
        if !self.manifest[i].log
            && !matches!(cause, Cause::Invalid | Cause::Limit)
            && to != State::Failed
        {
            return;
        }
        if self.count == LOG_SIZE {
            self.pop_event();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.journal[(self.head + self.count) % LOG_SIZE] = Some(Event {
            id: self.manifest[i].id,
            attempt,
            from,
            to,
            cause,
            exit,
            dependency,
        });
        self.count += 1;
    }

    fn transition(&mut self, i: usize, to: State, cause: Cause, exit: u64, dependency: u64) {
        let from = self.slots[i].state;
        self.slots[i].state = to;
        self.record(i, from, cause, exit, dependency);
    }

    fn check_attempt(&mut self, id: u64, attempt: u64) -> Result<usize, Error> {
        let i = self.registered_index(id)?;
        if attempt == 0 || self.slots[i].attempt != attempt {
            self.record(i, self.slots[i].state, Cause::Invalid, 0, 0);
            return Err(Error::Stale);
        }
        Ok(i)
    }

    fn invalid(&mut self, i: usize) -> Error {
        self.record(i, self.slots[i].state, Cause::Invalid, 0, 0);
        Error::InvalidState
    }

    fn closure(&self, i: usize, allowed: u16) -> u16 {
        let mut mask = 1 << i;
        for &j in self.order[..self.manifest.len()].iter().rev() {
            if mask & (1 << j) != 0 {
                mask |= (self.required[j] | self.optional[j]) & allowed;
            }
        }
        mask
    }

    /// Le client désigne une déclaration de confiance par son ID. Les dépendances
    /// du même catalogue sont enregistrées atomiquement dans l'ordre du graphe.
    pub fn register(&mut self, id: u64) -> Result<(), Error> {
        let mask = self.closure(self.index(id)?, u16::MAX);
        for pos in 0..self.manifest.len() {
            let i = self.order[pos];
            if mask & (1 << i) != 0 && !self.slots[i].registered {
                self.slots[i].registered = true;
                self.record(i, State::Registered, Cause::Registered, 0, 0);
            }
        }
        Ok(())
    }

    pub fn start(&mut self, id: u64) -> Result<(), Error> {
        let target = self.registered_index(id)?;
        let mask = self.closure(target, self.implicit_starts);
        for pos in 0..self.manifest.len() {
            let i = self.order[pos];
            if mask & (1 << i) == 0 {
                continue;
            }
            self.slots[i].wanted = true;
            match self.slots[i].state {
                State::Registered | State::Stopped | State::Failed => {
                    self.transition(i, State::Waiting, Cause::Requested, 0, 0);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn request_stop(&mut self, i: usize) {
        self.slots[i].wanted = false;
        if self.slots[i].state.live() {
            // L'ordre explicite l'emporte sur toute relance déjà décidée.
            self.slots[i].stop = Some(Cause::Requested);
            self.slots[i].dependency = 0;
        } else if self.slots[i].state != State::Stopped {
            self.transition(i, State::Stopped, Cause::Requested, 0, 0);
        }
    }

    pub fn stop(&mut self, id: u64) -> Result<(), Error> {
        let target = self.registered_index(id)?;
        let mut mask = 1 << target;
        for pos in 0..self.manifest.len() {
            let i = self.order[pos];
            if self.slots[i].registered && (i == target || self.required[i] & mask != 0) {
                mask |= 1 << i;
                self.request_stop(i);
            }
        }
        Ok(())
    }

    pub fn shutdown(&mut self) {
        for i in 0..self.manifest.len() {
            if self.slots[i].registered {
                self.request_stop(i);
            }
        }
    }

    pub fn restart(&mut self, id: u64) -> Result<(), Error> {
        let i = self.registered_index(id)?;
        self.start(id)?;
        if self.slots[i].state.live() {
            self.slots[i].stop = Some(Cause::Restart);
            self.slots[i].dependency = 0;
        }
        Ok(())
    }

    pub fn ready(&mut self, id: u64, attempt: u64, now: u64) -> Result<(), Error> {
        let i = self.check_attempt(id, attempt)?;
        if self.slots[i].state != State::Starting || self.slots[i].stop.is_some() {
            return Err(self.invalid(i));
        }
        if now >= self.slots[i].deadline {
            self.slots[i].stop = Some(Cause::StartTimeout);
            return Err(self.invalid(i));
        }
        self.refresh_heartbeat(i, now);
        self.transition(i, State::Ready, Cause::Available, 0, 0);
        Ok(())
    }

    fn refresh_heartbeat(&mut self, i: usize, now: u64) {
        self.slots[i].deadline = now.saturating_add(self.manifest[i].heartbeat_ticks);
    }

    pub fn heartbeat(&mut self, id: u64, attempt: u64, now: u64) -> Result<(), Error> {
        let i = self.check_attempt(id, attempt)?;
        if !self.slots[i].state.available() || self.slots[i].stop.is_some() {
            return Err(self.invalid(i));
        }
        if self.manifest[i].heartbeat_ticks != 0 && now >= self.slots[i].deadline {
            self.slots[i].stop = Some(Cause::Communication);
            return Err(self.invalid(i));
        }
        self.refresh_heartbeat(i, now);
        Ok(())
    }

    pub fn fault(&mut self, id: u64, attempt: u64, cause: Cause, _now: u64) -> Result<(), Error> {
        let i = self.check_attempt(id, attempt)?;
        if !self.slots[i].state.live() || !matches!(cause, Cause::Communication | Cause::Capability)
        {
            return Err(self.invalid(i));
        }
        self.slots[i].stop.get_or_insert(cause);
        Ok(())
    }

    pub fn finished(
        &mut self,
        id: u64,
        attempt: u64,
        cause: Cause,
        exit: u64,
        now: u64,
    ) -> Result<(), Error> {
        let i = self.check_attempt(id, attempt)?;
        let s = self.slots[i];
        if !s.state.live()
            || !matches!(
                cause,
                Cause::Exited | Cause::Crash | Cause::Killed | Cause::LaunchError
            )
        {
            return Err(self.invalid(i));
        }
        let reason = s.stop.unwrap_or(cause);
        let failed = !matches!(
            reason,
            Cause::Exited | Cause::Requested | Cause::Restart | Cause::Dependency
        ) || exit != 0;
        let state = if s.wanted && reason == Cause::Dependency {
            State::Waiting
        } else if failed && !matches!(reason, Cause::Requested | Cause::Restart) {
            State::Failed
        } else {
            State::Stopped
        };
        self.slots[i].stop = None;
        self.slots[i].dependency = 0;
        self.transition(i, state, reason, exit, s.dependency);
        if !s.wanted || state == State::Waiting {
            return Ok(());
        }
        if matches!(reason, Cause::Restart | Cause::Requested) {
            self.transition(i, State::Waiting, Cause::Requested, 0, 0);
            return Ok(());
        }
        let d = self.manifest[i];
        if d.restart == Restart::Always || (failed && d.restart == Restart::Failure) {
            let slot = &mut self.slots[i];
            let old =
                slot.restarts[..slot.used].partition_point(|&t| now.saturating_sub(t) >= d.window);
            slot.restarts.copy_within(old..slot.used, 0);
            slot.used -= old;
            if slot.used >= usize::from(d.retries) {
                self.transition(i, State::Failed, Cause::Limit, exit, 0);
            } else {
                let delay = d.backoff.saturating_mul(1 << slot.used);
                slot.restarts[slot.used] = now;
                slot.used += 1;
                slot.deadline = now.saturating_add(delay);
                self.transition(i, State::Restarting, Cause::Restart, exit, 0);
            }
        }
        Ok(())
    }

    fn unavailable(&self, i: usize) -> Option<usize> {
        (0..self.manifest.len()).find(|&j| {
            self.required[i] & (1 << j) != 0
                && (!self.slots[j].state.available() || self.slots[j].stop.is_some())
        })
    }

    // En arrêt système, l'ordre inverse inclut aussi les liens optionnels.
    // Une perte isolée d'un fournisseur optionnel laisse fonctionner ses clients.
    fn has_stopping_dependent(&self, i: usize) -> bool {
        (0..self.manifest.len()).any(|j| {
            (self.required[j] | self.optional[j]) & (1 << i) != 0
                && self.slots[j].state.live()
                && self.slots[j].stop.is_some()
        })
    }

    /// Appeler jusqu'à None, puis attendre un événement ou la prochaine échéance.
    /// Chaque lancement réserve un essai distinct avant de remettre l'action.
    pub fn next_action(&mut self, now: u64) -> Option<Action> {
        for pos in 0..self.manifest.len() {
            let i = self.order[pos];
            let s = self.slots[i];
            if !s.registered {
                continue;
            }
            if s.state == State::Restarting && now >= s.deadline {
                self.transition(i, State::Waiting, Cause::Restart, 0, 0);
            }
            if s.state == State::Ready && s.stop.is_none() {
                self.transition(i, State::Running, Cause::Available, 0, 0);
            }
            if !s.state.live() || s.stop.is_some() {
                continue;
            }
            if let Some(dep) = self.unavailable(i) {
                self.slots[i].stop = Some(Cause::Dependency);
                self.slots[i].dependency = self.manifest[dep].id;
            } else if s.state == State::Starting && now >= s.deadline {
                self.slots[i].stop = Some(Cause::StartTimeout);
            } else if s.state.available()
                && self.manifest[i].heartbeat_ticks != 0
                && now >= s.deadline
            {
                self.slots[i].stop = Some(Cause::Communication);
            }
        }
        for pos in (0..self.manifest.len()).rev() {
            let i = self.order[pos];
            let s = self.slots[i];
            let Some(reason) = s.stop else {
                continue;
            };
            let id = self.manifest[i].id;
            let attempt = s.attempt;
            if s.state == State::Stopping {
                if !s.killed && now >= s.deadline {
                    self.slots[i].killed = true;
                    self.record(i, s.state, Cause::StopTimeout, 0, s.dependency);
                    return Some(Action::Kill { id, attempt });
                }
            } else if !self.has_stopping_dependent(i) {
                self.slots[i].deadline = now.saturating_add(self.manifest[i].stop_ticks);
                self.transition(i, State::Stopping, reason, 0, s.dependency);
                return Some(Action::Stop { id, attempt });
            }
        }
        for pos in 0..self.manifest.len() {
            let i = self.order[pos];
            let s = self.slots[i];
            if !s.wanted || s.state != State::Waiting {
                continue;
            }
            if let Some(dep) = self.unavailable(i) {
                // Le démarrage d'une dépendance en cours maintient WAITING.
                // Un fournisseur en échec terminal requiert une intervention.
                if matches!(self.slots[dep].state, State::Failed | State::Stopped) {
                    self.transition(
                        i,
                        State::Failed,
                        Cause::Dependency,
                        0,
                        self.manifest[dep].id,
                    );
                }
                continue;
            }
            let Some(attempt) = s.attempt.checked_add(1) else {
                self.transition(i, State::Failed, Cause::Limit, 0, 0);
                continue;
            };
            self.slots[i].attempt = attempt;
            self.slots[i].deadline = now.saturating_add(self.manifest[i].start_ticks);
            self.slots[i].killed = false;
            self.transition(i, State::Starting, Cause::Started, 0, 0);
            return Some(Action::Launch {
                id: self.manifest[i].id,
                attempt,
            });
        }
        None
    }
}
