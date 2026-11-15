//! Décisions du journal, partagées entre le service invité et les essais de protocole.
use crate::{
    access, file,
    journal_backend::{Client, Status},
    journal_wire as w, runtime as rt,
};
use access::Filter;
const CACHE_SLOTS: usize = 32;

const WATCH_TICKS: u64 = 62;
struct Watch {
    tag: u64,
    filter: Filter,
    continuation: u64,
    ticks: u64,
    turns: u64,
}
pub struct Journal {
    backend: Client,
    next: Option<u64>,
    metadata: Option<Status>,
    cache: [w::Event; CACHE_SLOTS],
    shell: w::Identity,
    manager: w::Identity,
    rights: u64,
    last_notice: u64,
    pending: Option<Watch>,
    // Point de reprise interne, jamais exposé au lecteur OWN.
    own_scan: Option<(Filter, u64)>,
    terminal_tx: u64,
    #[cfg(journal_pannes)]
    probe: crate::probe::Probe,
}

pub fn reply(op: u64, status: u64, value: u64) -> w::Frame {
    let mut f = w::request(op);
    f[2] = status;
    f[3] = value;
    f
}

pub fn respond(tx: u64, tag: u64, frame: &w::Frame) -> rt::Result<()> {
    rt::send_frame(tx, tag, &w::encode(frame), 0, false)
}

impl Journal {
    pub fn new(
        backend: Client,
        shell: w::Identity,
        manager: w::Identity,
        rights: u64,
        terminal_tx: u64,
    ) -> Self {
        Self {
            backend,
            shell,
            manager,
            rights,
            terminal_tx,
            #[cfg(journal_pannes)]
            probe: crate::probe::Probe::default(),
            next: None,
            metadata: None,
            last_notice: 0,
            pending: None,
            own_scan: None,
            cache: [[0; 12]; CACHE_SLOTS],
        }
    }

    #[cfg(journal_pannes)]
    pub fn acknowledged(&mut self, marker: u64) {
        if self.rights == 63 {
            self.probe.after_ack(marker);
        }
    }

    pub fn drop_client(&mut self) {
        self.pending = None;
    }

    pub fn advance(&mut self, slept: u64) {
        if let Some(watch) = self.pending.as_mut() {
            watch.ticks += slept;
            watch.turns += 1;
            if watch.ticks >= WATCH_TICKS || watch.turns >= 64 {
                self.finish_watch();
            }
        }
    }

    pub fn refresh(&mut self) -> rt::Result<Status> {
        self.metadata = None;
        self.next = None;
        let s = self.backend.status()?;
        if s.first == 0
            || s.first > s.next
            || s.next - s.first > 128
            || s.bytes != (s.next - s.first) * 96
        {
            return Err(rt::REFUSED);
        }
        self.metadata = Some(s);
        self.next = Some(s.next);
        Ok(s)
    }

    fn status(&mut self) -> rt::Result<Status> {
        match self.metadata {
            Some(s) => Ok(s),
            None => self.refresh(),
        }
    }

    pub fn append(&mut self, mut event: w::Event) -> rt::Result<u64> {
        #[cfg(journal_pannes)]
        self.probe.check_write()?;
        let expected = match self.next {
            Some(n) => n,
            None => self.refresh()?.next,
        };
        event[1] = expected;
        self.metadata = None;
        self.next = None;
        let seq = self.backend.append(expected, &w::bytes(&event))?;
        self.next = if seq == expected {
            expected.checked_add(1)
        } else {
            Some(expected)
        };
        event[1] = seq;
        self.cache[seq as usize % CACHE_SLOTS] = event;
        if seq == expected {
            if let Some(watch) = self.pending.as_mut() {
                let next_global = seq.saturating_add(1);
                if let Some((visible, cursor)) = watch.filter.visible(&event, self.shell) {
                    if watch.filter.service == w::SELF {
                        let mut next_filter = watch.filter;
                        next_filter.cursor = cursor;
                        self.own_scan = Some((next_filter, next_global));
                    }
                    let mut response = reply(w::WATCH, rt::OK, cursor);
                    response[4..].copy_from_slice(&visible);
                    let _ = respond(self.terminal_tx, watch.tag, &response);
                    self.pending = None;
                } else if watch.filter.service == w::SELF {
                    self.own_scan = Some((watch.filter, next_global));
                } else {
                    watch.continuation = watch.continuation.max(next_global);
                    watch.filter.cursor = watch.continuation;
                }
            }
        }
        Ok(seq)
    }

    fn record(&mut self, seq: u64) -> rt::Result<w::Event> {
        let slot = seq as usize % CACHE_SLOTS;
        if self.cache[slot][1] != seq {
            let event = w::from_bytes(&self.backend.read(seq)?);
            if !w::valid(&event) || event[1] != seq {
                return Err(rt::REFUSED);
            }
            self.cache[slot] = event;
        }
        Ok(self.cache[slot])
    }

    fn search(&mut self, op: u64, filter: Filter) -> rt::Result<w::Frame> {
        let s = self.status()?;
        let own = filter.service == w::SELF;
        let cursor = filter.cursor;
        if cursor == w::END {
            return Ok(reply(op, rt::WOULD_BLOCK, w::END));
        }
        if !own && cursor != 0 && cursor < s.first {
            return Ok(reply(op, rt::REVOKED, s.first));
        }
        let mut low = s.first;
        let mut high = s.next;
        if own
            && !filter.reverse
            && let Some((previous, position)) = self.own_scan
            && previous == filter
        {
            low = low.max(position);
        }
        if !own && cursor != 0 {
            if filter.reverse {
                high = high.min(cursor.saturating_add(1));
            } else {
                low = low.max(cursor);
            }
        }
        while low < high {
            let seq = if filter.reverse {
                high -= 1;
                high
            } else {
                let n = low;
                low += 1;
                n
            };
            if let Some((record, next)) = filter.visible(&self.record(seq)?, self.shell) {
                if own && !filter.reverse {
                    let mut resume = filter;
                    resume.cursor = next;
                    self.own_scan = Some((resume, seq.saturating_add(1)));
                }
                let mut response = reply(op, rt::OK, next);
                response[4..].copy_from_slice(&record);
                return Ok(response);
            }
        }
        if own && !filter.reverse {
            self.own_scan = Some((filter, s.next));
        }
        let next = if filter.reverse {
            w::END
        } else if own {
            cursor
        } else {
            s.next.max(cursor)
        };
        Ok(reply(op, rt::WOULD_BLOCK, next))
    }

    pub fn terminal(&mut self, f: &w::Frame, tag: u64) -> rt::Result<Option<w::Frame>> {
        access::client_right(self.rights, f)?;
        let op = f[1];
        match op {
            w::STATUS if f[3..].iter().all(|&v| v == 0) => {
                let s = self.refresh()?;
                let mut answer = reply(op, rt::OK, 0);
                answer[7] = s.revision;
                if self.rights & w::SYSTEM_READ != 0 {
                    answer[3..7].copy_from_slice(&[s.first, s.next, s.segments, s.retention]);
                    answer[8..12].copy_from_slice(&[
                        s.quota,
                        s.bytes,
                        self.manager[0],
                        self.manager[1],
                    ]);
                } else {
                    answer[13] = 1;
                }
                Ok(Some(answer))
            }
            w::NEXT | w::WATCH => {
                let mut filter = Filter::parse(f)?;
                if op == w::WATCH && self.pending.is_some() {
                    return Err(file::BUSY);
                }
                let answer = self.search(op, filter)?;
                if op == w::WATCH && answer[2] == rt::WOULD_BLOCK && answer[3] != w::END {
                    filter.cursor = answer[3];
                    self.pending = Some(Watch {
                        tag,
                        filter,
                        continuation: answer[3],
                        ticks: 0,
                        turns: 0,
                    });
                    return Ok(None);
                }
                Ok(Some(answer))
            }
            w::ROTATE | w::RETAIN | w::PURGE => {
                if f[5..].iter().any(|&n| n != 0) || (op != w::RETAIN && f[4] != 0) {
                    return Err(rt::BAD_ARGUMENT);
                }
                #[cfg(journal_pannes)]
                self.probe.check_admin(op)?;
                self.metadata = None;
                self.next = None;
                let revision = match op {
                    w::ROTATE => self.backend.rotate(f[3])?,
                    w::PURGE => self.backend.purge(f[3])?,
                    _ => self.backend.retain(f[3], f[4])?,
                };
                // Une rotation/purge peut retirer le curseur du lecteur pendant
                // l'attente. Le lecteur redemandera les bornes après le
                // WOULD_BLOCK ; le prochain NEXT contrôlera le curseur exact.
                self.finish_watch();
                Ok(Some(reply(op, rt::OK, revision)))
            }
            w::SELF_NOTICE => {
                let event = access::notice(f, self.shell)?;
                #[cfg(journal_pannes)]
                if self.rights == 63 && f[4] == 241 {
                    crate::probe::crash();
                }
                if event[0] <= self.last_notice {
                    return Err(file::BUSY);
                }
                // Une admission dont l'issue est incertaine consomme aussi cet EID.
                self.last_notice = event[0];
                #[cfg(journal_pannes)]
                if self.rights == 63 && f[4] == 242 {
                    crate::probe::crash();
                }
                self.append(event)?;
                #[cfg(journal_pannes)]
                if self.rights == 63 && f[4] == 243 {
                    crate::probe::crash();
                }
                Ok(Some(reply(op, rt::OK, self.last_notice)))
            }
            _ => Err(rt::BAD_ARGUMENT),
        }
    }

    fn finish_watch(&mut self) {
        if let Some(watch) = self.pending.take() {
            let _ = respond(
                self.terminal_tx,
                watch.tag,
                &reply(w::WATCH, rt::WOULD_BLOCK, watch.continuation),
            );
        }
    }
}
