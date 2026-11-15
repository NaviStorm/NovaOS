//! Validation des deux canaux. L'identité et le masque proviennent du bootstrap.
use crate::{journal_wire as w, runtime as rt};

pub fn client_right(mask: u64, frame: &w::Frame) -> rt::Result<()> {
    let needed = match frame[1] {
        w::NEXT | w::WATCH if frame[4] == w::SELF => w::OWN | w::SYSTEM_READ,
        w::NEXT | w::WATCH => w::SYSTEM_READ,
        w::STATUS => w::SYSTEM_READ | w::ROTATION_RIGHT | w::RETENTION_RIGHT | w::PURGE_RIGHT,
        w::ROTATE => w::ROTATION_RIGHT,
        w::RETAIN => w::RETENTION_RIGHT,
        w::PURGE => w::PURGE_RIGHT,
        w::SELF_NOTICE => w::EMIT_RIGHT,
        _ => return Err(rt::DENIED),
    };
    if mask & needed == 0 {
        return Err(rt::DENIED);
    }
    if frame[0] != w::VERSION || frame[2] != 0 {
        return Err(rt::BAD_ARGUMENT);
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Filter {
    pub cursor: u64,
    pub service: u64,
    level: u8,
    boot: [u64; 2],
    pub reverse: bool,
}
impl Filter {
    pub fn parse(f: &w::Frame) -> rt::Result<Self> {
        if (f[4] > u16::MAX as u64 && f[4] != w::SELF)
            || f[5] > u64::from(w::CRITICAL)
            || f[8] > 1
            || (f[1] == w::WATCH && f[8] != 0)
            || f[9..].iter().any(|&n| n != 0)
        {
            return Err(rt::BAD_ARGUMENT);
        }
        Ok(Self {
            cursor: f[3],
            service: f[4],
            level: f[5] as u8,
            boot: [f[6], f[7]],
            reverse: f[8] == 1,
        })
    }

    /// Un lecteur OWN ne voit que les EID de son incarnation. Un filtre sur une
    /// autre identité n'accède ni aux évènements ni aux compteurs du système.
    pub fn visible(&self, event: &w::Event, principal: w::Identity) -> Option<(w::Event, u64)> {
        if w::level(event) < self.level || (self.boot != [0; 2] && self.boot != event[2..4]) {
            return None;
        }
        let mut record = *event;
        if self.service == w::SELF {
            if w::issuer(event) != principal {
                return None;
            }
            record[1] = event[0];
        } else if self.service != 0 && self.service != w::service(event) {
            return None;
        }
        let seq = record[1];
        if self.cursor != 0
            && ((self.reverse && seq > self.cursor) || (!self.reverse && seq < self.cursor))
        {
            return None;
        }
        let cursor = if self.reverse {
            if seq == 1 { w::END } else { seq - 1 }
        } else {
            seq.saturating_add(1)
        };
        Some((record, cursor))
    }
}

pub fn manager_event(frame: &w::Frame, manager: w::Identity) -> rt::Result<w::Event> {
    if frame[..4] != [w::VERSION, w::EMIT, 0, 0] {
        return Err(rt::BAD_ARGUMENT);
    }
    let event: w::Event = frame[4..].try_into().unwrap();
    if !w::valid(&event) || event[1] != 0 {
        return Err(rt::BAD_ARGUMENT);
    }
    let issuer = w::issuer(&event);
    if issuer[..2] != manager[..2] {
        return Err(rt::DENIED);
    }
    match w::origin(&event) {
        w::MANAGER
            if issuer == manager
                && w::code(&event) != w::NOTICE
                && w::code(&event) != w::ONLINE =>
        {
            Ok(event)
        }
        // Le gestionnaire atteste ici une notification réellement lue sur le
        // canal de l'incarnation. Il ne peut en faire un événement de contrôle.
        w::PRODUCER
            if (1..w::JOURNAL_ID).contains(&w::service(&event))
                && issuer != manager
                && issuer == w::subject(&event)
                && w::code(&event) == w::NOTICE
                && w::category(&event) == w::SERVICE =>
        {
            Ok(event)
        }
        _ => Err(rt::DENIED),
    }
}

pub fn notice(frame: &w::Frame, principal: w::Identity) -> rt::Result<w::Event> {
    if frame[3] == 0 || frame[3] == w::END || frame[4] > 255 || frame[5..].iter().any(|&n| n != 0) {
        return Err(rt::BAD_ARGUMENT);
    }
    let mut event: w::Event = w::event(
        frame[3],
        principal,
        principal,
        0,
        w::INFO,
        w::SERVICE,
        w::PRODUCER,
        w::NOTICE,
        0,
        [frame[4], 0],
    )[4..]
        .try_into()
        .unwrap();
    // Le client ne fournit ni horloge, ni contenu libre, ni identité.
    event[8] = w::meta(
        0,
        w::INFO,
        w::SERVICE,
        w::PRODUCER,
        w::NOTICE,
        w::UNKNOWN_TIME,
    );
    Ok(event)
}
