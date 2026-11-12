//! Un endpoint privé donne accès à un seul volume et à sa table locale de handles.
use crate::{device::IpcDevice, protocol as w, runtime as rt};
use alloc::sync::Arc;
use alloc::{string::String, vec::Vec};
use core::sync::atomic::{AtomicI64, Ordering};
use novafs::time::{Clock, Timestamp};
use novafs::{Device, Engine, Error, Kind, Mode, View, codec::Anchor};
use w::Frame;

struct Civil(AtomicI64);

impl Civil {
    fn sample(&self) -> rt::Result<Timestamp> {
        let seconds = rt::value(rt::call(rt::CLOCK_UTC, 0, 0, 0))?;
        let seconds = i64::try_from(seconds).map_err(|_| rt::REFUSED)?;
        self.0.store(seconds, Ordering::Relaxed);
        Timestamp::utc(seconds, 0).map_err(error)
    }
}

impl Clock for Civil {
    fn now(&self) -> Timestamp {
        Timestamp::utc(self.0.load(Ordering::Relaxed), 0).unwrap()
    }
}

fn error(e: Error) -> u64 {
    match e {
        Error::UnknownOutcome | Error::Publication { .. } => w::UNCERTAIN,
        Error::Unavailable | Error::Io(_) => w::IO,
        Error::Corrupt(_) => w::CORRUPT,
        Error::Invalid(_) | Error::NotDirectory | Error::IsDirectory => rt::BAD_ARGUMENT,
        Error::Limit(_) => rt::LIMIT,
        Error::NoSpace | Error::PoolFull | Error::QuotaExceeded | Error::MaintenanceExhausted => {
            w::FULL
        }
        Error::NoMemory => rt::NO_MEMORY,
        Error::NotFound => w::NOT_FOUND,
        Error::ReadOnly => rt::DENIED,
        _ => w::BUSY,
    }
}
struct File {
    key: u64,
    id: u64,
    kind: Kind,
    rights: u8,
}
struct Pending {
    file: u64,
    offset: u64,
    size: usize,
    reserved: u64,
    bytes: Vec<u8>,
}
#[derive(Default)]
struct PathInput {
    total: usize,
    bytes: Vec<u8>,
}
struct Client {
    id: u64,
    mounted: bool,
    closed: bool,
    scoped: bool,
    rights: u8,
    cwd: (u64, u8),
    handles: Vec<File>,
    paths: [PathInput; 2],
    targets: Vec<selected::Target>,
    copy: Option<(u64, String, u64)>,
}
impl Client {
    fn new(id: u64, rights: u8) -> Self {
        Self {
            id,
            mounted: false,
            closed: false,
            scoped: false,
            rights,
            cwd: (0, 0),
            handles: Vec::new(),
            paths: Default::default(),
            targets: Vec::new(),
            copy: None,
        }
    }
    fn handle(&self, key: u64) -> rt::Result<&File> {
        self.handles
            .iter()
            .find(|f| f.key == key)
            .ok_or(rt::REVOKED)
    }
}
struct Session<'a, 'b> {
    dev: &'a IpcDevice<'b>,
    sector: u32,
    writable: bool,
    fs: Option<Engine<&'a IpcDevice<'b>>>,
    clock: Arc<Civil>,
    next: u64,
    pending: Option<Pending>,
    failed: Option<u64>,
}
fn shape(ok: bool) -> rt::Result<()> {
    if ok { Ok(()) } else { Err(rt::BAD_ARGUMENT) }
}
impl Session<'_, '_> {
    fn dispatch(
        &mut self,
        client: &mut Client,
        other: Option<&Client>,
        q: &Frame,
        a: &mut Frame,
    ) -> rt::Result<()> {
        let op = q[1];
        let flags = q[2];
        let n = w::length(q);
        let session = w::word(q, 8);
        let handle = w::word(q, 16);
        let offset = w::word(q, 24);
        let total = w::word(q, 32);
        shape(q[0] == w::VERSION && q[3] == 0 && q[6..8] == [0; 2] && n <= w::FRAGMENT)?;
        let content = if matches!(
            op,
            w::WRITE
                | w::OPEN
                | w::PATH_FRAGMENT
                | w::DIR_OPEN
                | w::OPEN_AT
                | w::LOOKUP_AT
                | w::MKDIR_AT
                | w::RMDIR
                | w::UNLINK_AT
                | w::READDIR
        ) {
            n
        } else {
            0
        };
        shape(q[w::PAYLOAD + content..].iter().all(|b| *b == 0))?;
        let plain = flags == 0 && n == 0 && offset == 0 && total == 0;
        if let Some(e) = self.failed {
            return Err(e);
        }
        if op == w::MOUNT {
            shape(plain && session == 0 && handle == 0)?;
            if client.mounted {
                return Err(w::BUSY);
            }
            if self.fs.is_none() {
                let first = self.dev.read(0).map_err(error)?;
                if &first[..8] != b"NOVAP0FS" {
                    return Err(w::UNSUPPORTED);
                }
                if rt::config(1) == 23 {
                    crate::qualification::stop(b"COUPURE remontage");
                }
                let ids = Anchor::decode(&first, 0).map_err(error)?.ids;
                let mut lv = [0u8; 16];
                if rt::config(1) == 41 {
                    lv[..8].copy_from_slice(&rt::config(2).to_le_bytes());
                    lv[8..].copy_from_slice(&rt::config(3).to_le_bytes());
                }
                if lv != [0; 16] {
                    let mut fs_id = lv;
                    fs_id[15] ^= 1;
                    if ids != (novafs::Ids { fs: fs_id, lv }) {
                        return Err(w::CORRUPT);
                    }
                    let check =
                        Engine::mount_current(self.dev, ids, Mode::ReadOnly).map_err(error)?;
                    if check.atime_policy() != Some(novafs::time::Atime::Relative) {
                        return Err(w::UNSUPPORTED);
                    }
                    check.unmount().map_err(error)?;
                }
                let mode = if self.writable {
                    Mode::ReadWrite
                } else {
                    Mode::ReadOnly
                };
                let mut fs = Engine::mount_current(self.dev, ids, mode).map_err(error)?;
                if self.writable && fs.atime_policy().is_some() {
                    self.clock.sample()?;
                    fs.set_clock(self.clock.clone());
                }
                self.fs = Some(fs);
            }
            client.cwd = (self.fs.as_ref().unwrap().root_id(), client.rights);
            client.mounted = true;
            w::set(a, 8, client.id);
            return Ok(());
        }
        if session != client.id || !client.mounted {
            return Err(rt::DENIED);
        }
        let needs_date = matches!(
            op,
            w::READ | w::READDIR | w::MKDIR_AT | w::RMDIR | w::UNLINK_AT | w::RENAME_AT | w::SYNC
        ) || matches!(op, w::OPEN | w::OPEN_AT) && flags & w::CREATE != 0;
        if needs_date
            && self.writable
            && self
                .fs
                .as_ref()
                .is_some_and(|fs| fs.atime_policy().is_some())
        {
            self.clock.sample()?;
        }
        if (w::PATH_FRAGMENT..=w::GETCWD).contains(&op) {
            return self.directory_request(client, other, q, a);
        }
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        match op {
            w::INFO => {
                shape(plain && handle == 0)?;
                let stats = fs.accounting().map_err(error)?;
                let available = stats
                    .available_blocks
                    .checked_sub(self.pending.as_ref().map_or(0, |p| p.reserved))
                    .ok_or(w::CORRUPT)?;
                let p = &mut a[w::PAYLOAD..];
                p[..16].copy_from_slice(&fs.ids().lv);
                p[16..32].copy_from_slice(&fs.ids().fs);
                p[32..36].copy_from_slice(&self.sector.to_le_bytes());
                p[36..40].copy_from_slice(&4096u32.to_le_bytes());
                p[40..48].copy_from_slice(&stats.capacity_blocks.to_le_bytes());
                p[48..56].copy_from_slice(&available.to_le_bytes());
                p[56..72].copy_from_slice(&stats.logical_bytes.to_le_bytes());
                p[72..80].copy_from_slice(&fs.generation().to_le_bytes());
                p[80] = if client.rights & w::FILE_WRITE != 0 {
                    3
                } else {
                    1
                };
                p[81] = if fs.atime_policy().is_some() { 9 } else { 0 };
                p[82] = 1; // Transactions CoW de fichiers.
                w::size(a, 83);
            }
            w::OPEN => {
                shape(
                    handle == 0
                        && offset == 0
                        && total == 0
                        && n > 0
                        && flags & !7 == 0
                        && flags & 3 != 0,
                )?;
                if flags & w::CREATE != 0 && flags & w::WRITABLE == 0
                    || client.rights & w::FILE_WRITE == 0 && flags & w::WRITABLE != 0
                    || client.rights & w::FILE_READ == 0 && flags & w::READABLE != 0
                    || client.rights & w::ADD == 0 && flags & w::CREATE != 0
                {
                    return Err(rt::DENIED);
                }
                if client.handles.len() == 32 {
                    return Err(rt::LIMIT);
                }
                let next = self
                    .next
                    .checked_add(1)
                    .filter(|v| *v < 1u64 << 63)
                    .ok_or(rt::LIMIT)?;
                let path = core::str::from_utf8(&q[w::PAYLOAD..w::PAYLOAD + n])
                    .map_err(|_| rt::BAD_ARGUMENT)?;
                shape(
                    path.starts_with('/')
                        && !path.bytes().any(|b| b < 32 || b == 127)
                        && path[1..].split('/').all(|s| !matches!(s, "" | "." | "..")),
                )?;
                client.handles.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
                let info = match fs.stat(path) {
                    Ok(info) => info,
                    Err(Error::NotFound) if flags & w::CREATE != 0 => {
                        if self.pending.is_some() {
                            return Err(w::BUSY);
                        }
                        fs.create_at(fs.root_id(), &path[1..], Kind::File)
                            .map_err(error)?
                    }
                    Err(e) => return Err(error(e)),
                };
                shape(info.kind == Kind::File)?;
                self.next = next;
                client.handles.push(File {
                    key: next,
                    id: info.id,
                    kind: info.kind,
                    rights: flags & 3,
                });
                w::set(a, 16, next);
            }
            w::CLOSE if handle == 0 => {
                shape(plain)?;
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|p| client.handle(p.file).is_ok())
                {
                    return Err(w::BUSY);
                }
                if other.is_none_or(|c| c.closed) {
                    if self.writable {
                        while fs.collection_pending().map_err(error)? {
                            fs.collect().map_err(error)?;
                        }
                    }
                    self.fs
                        .take()
                        .unwrap()
                        .unmount()
                        .map_err(error)?
                        .close()
                        .map_err(error)?;
                }
            }
            w::READ | w::WRITE | w::SYNC | w::CLOSE => {
                let at = client
                    .handles
                    .iter()
                    .position(|f| f.key == handle)
                    .ok_or(rt::REVOKED)?;
                let file = &client.handles[at];
                shape(file.kind == Kind::File)?;
                match op {
                    w::READ => {
                        shape(flags == 0 && total == 0 && offset.checked_add(n as u64).is_some())?;
                        if file.rights & w::READABLE == 0 {
                            return Err(rt::DENIED);
                        }
                        if self.pending.is_some()
                            && fs
                                .atime_policy()
                                .is_some_and(|p| p != novafs::time::Atime::No)
                        {
                            return Err(w::BUSY);
                        }
                        let bytes = fs.read_access_id(file.id, offset, n).map_err(error)?;
                        a[w::PAYLOAD..w::PAYLOAD + bytes.len()].copy_from_slice(&bytes);
                        let mut count = bytes.len();
                        if let Some(p) = self
                            .pending
                            .as_ref()
                            .filter(|p| p.file == handle && p.bytes.len() == p.size)
                        {
                            // Le reste de la réponse a été initialisé à zéro, y compris un éventuel trou.
                            let end = p.offset + p.size as u64;
                            count = count.max(end.saturating_sub(offset).min(n as u64) as usize);
                            let start = offset.max(p.offset);
                            let stop = (offset + n as u64).min(end);
                            if start < stop {
                                let dst = w::PAYLOAD + (start - offset) as usize;
                                a[dst..dst + (stop - start) as usize].copy_from_slice(
                                    &p.bytes
                                        [(start - p.offset) as usize..(stop - p.offset) as usize],
                                );
                            }
                        }
                        w::size(a, count);
                    }
                    w::WRITE => {
                        if file.rights & w::WRITABLE == 0 {
                            return Err(rt::DENIED);
                        }
                        shape(
                            n > 0
                                && total <= w::MAX_WRITE as u64
                                && n as u64 <= total
                                && matches!(flags, w::START | w::CONTINUE),
                        )?;
                        if flags == w::START {
                            shape(
                                offset
                                    .checked_add(total)
                                    .is_some_and(|end| end <= i64::MAX as u64),
                            )?;
                            if self.pending.is_some() {
                                return Err(w::BUSY);
                            }
                            let mut bytes = Vec::new();
                            bytes
                                .try_reserve_exact(total as usize)
                                .map_err(|_| rt::NO_MEMORY)?;
                            self.pending = Some(Pending {
                                file: handle,
                                offset,
                                size: total as usize,
                                reserved: 0,
                                bytes,
                            });
                        }
                        let p = self.pending.as_mut().ok_or(rt::BAD_ARGUMENT)?;
                        shape(
                            p.file == handle
                                && total as usize == p.size
                                && p.offset.checked_add(p.bytes.len() as u64) == Some(offset)
                                && n <= p.size - p.bytes.len(),
                        )?;
                        p.bytes.extend_from_slice(&q[w::PAYLOAD..w::PAYLOAD + n]);
                        if p.bytes.len() == p.size {
                            // Le seul écrivain reste à cette génération jusqu'à SYNC ou abandon.
                            // La réservation est évaluée par le moteur, y compris le coût des métadonnées.
                            match fs.prepare_write_id(file.id, p.offset, &p.bytes) {
                                Ok(plan) => {
                                    p.reserved = plan.accounting().transaction_reserved;
                                    plan.cancel();
                                }
                                Err(e) => {
                                    self.pending = None;
                                    return Err(error(e));
                                }
                            }
                        }
                        w::set(a, 32, p.bytes.len() as u64);
                    }
                    w::SYNC => {
                        shape(plain)?;
                        if file.rights & w::WRITABLE == 0 {
                            return Err(rt::DENIED);
                        }
                        if let Some(p) = &self.pending {
                            if p.file != handle || p.bytes.len() != p.size {
                                return Err(w::BUSY);
                            }
                            if rt::config(1) == 28 {
                                rt::exit(rt::CANCELLED)
                            }
                            self.dev.start_probe();
                            fs.prepare_write_id(file.id, p.offset, &p.bytes)
                                .map_err(error)?
                                .commit()
                                .map_err(error)?;
                            self.pending = None;
                        } else {
                            self.dev.flush().map_err(error)?;
                        }
                        w::set(a, 32, fs.generation());
                    }
                    _ => {
                        shape(
                            matches!(flags, 0 | w::DISCARD) && n == 0 && total == 0 && offset == 0,
                        )?;
                        if self.pending.as_ref().is_some_and(|p| p.file == handle) {
                            if flags != w::DISCARD {
                                return Err(w::BUSY);
                            }
                            self.pending = None;
                        }
                        if rt::config(1) == 22 && file.rights & w::WRITABLE != 0 {
                            crate::qualification::stop(b"COUPURE fermeture");
                        }
                        client.handles.swap_remove(at);
                    }
                }
            }
            _ => return Err(w::UNSUPPORTED),
        }
        Ok(())
    }
}
mod delegated;
mod namespace;
mod selected;

pub(crate) fn serve(dev: &IpcDevice<'_>, sector: u32, writable: bool) -> rt::Result<()> {
    let mut s = Session {
        dev,
        sector,
        writable,
        fs: None,
        clock: Arc::new(Civil(AtomicI64::new(0))),
        next: 0,
        pending: None,
        failed: None,
    };
    let pid = rt::value(rt::call(rt::PROCESS_ID, 0, 0, 0))?;
    let pair = rt::config(6) != 0;
    let mut clients = [
        Client::new(pid, if writable { w::DIR_ALL } else { w::DIR_RO }),
        Client::new(pid | (1 << 63), w::DIR_RO),
    ];
    let ports = [
        (rt::config(4), rt::config(5)),
        (rt::config(6), rt::config(7)),
    ];
    let mut last = [0; 2];
    let mut lane = 0;
    let cli = matches!(rt::config(1), 41 | 42);
    let mut admission = delegated::Ports::default();
    loop {
        let i = lane;
        if pair {
            lane ^= 1;
        }
        let (client, other) = if i == 0 {
            let [c, peer] = &mut clients;
            (c, pair.then_some(&*peer))
        } else {
            let [peer, c] = &mut clients;
            (c, Some(&*peer))
        };
        if client.closed {
            continue;
        }
        let timeout = if pair { 1 } else { 15000 };
        let m = match rt::receive_options(ports[i].0, timeout, u32::from(cli)) {
            Err(rt::TIMED_OUT) if cli => continue,
            Err(rt::TIMED_OUT) if pair => match rt::receive_for(rt::config(8 + i), 0) {
                Err(rt::WOULD_BLOCK) => continue,
                Err(rt::DENIED | rt::CLOSED | rt::REVOKED) => {
                    if s.pending
                        .as_ref()
                        .is_some_and(|p| client.handle(p.file).is_ok())
                    {
                        s.pending = None;
                    }
                    rt::value(rt::call(rt::IPC_CLOSE, ports[i].0, 0, 0))?;
                    client.closed = true;
                    client.handles.clear();
                    client.paths = Default::default();
                    if other.is_some_and(|c| c.closed) {
                        if let Some(fs) = s.fs.take() {
                            fs.unmount().map_err(error)?;
                        }
                        return s.dev.close().map_err(error);
                    }
                    continue;
                }
                Ok(_) => continue,
                Err(e) => return Err(e),
            },
            r => r?,
        };
        if cli && m.kind == 3 {
            admission.accept(m);
            continue;
        }
        let mut grant = None;
        let mut a = w::frame(m.payload[1], 0, 0);
        let r = if m.kind != 1 || m.len != 128 || m.tag <= last[i] {
            Err(rt::BAD_ARGUMENT)
        } else {
            last[i] = m.tag;
            if cli && m.payload[1] == w::DELEGATE {
                delegated::prepare(&s, client, &m.payload, &mut admission).map(|g| grant = Some(g))
            } else if cli && m.payload[1] == w::STAGE {
                selected::stage(&s, client, &m.payload)
            } else {
                s.dispatch(client, other, &m.payload, &mut a)
            }
        };
        if let Err(e) = r {
            a = w::frame(m.payload[1], 0, 0);
            a[3] = e as u8;
            if matches!(e, w::UNCERTAIN | w::IO | w::CORRUPT) {
                s.failed = Some(e);
            }
        }
        let done = r.is_ok() && m.payload[1] == w::CLOSE && w::word(&m.payload, 16) == 0;
        if done {
            rt::value(rt::call(rt::IPC_CLOSE, ports[i].0, 0, 0))?;
            client.closed = true;
            client.handles.clear();
            client.paths = Default::default();
        }
        if let Err(e) = rt::send(ports[i].1, m.tag, &a) {
            if !pair
                || !matches!(
                    rt::receive_for(rt::config(8 + i), 0),
                    Err(rt::DENIED | rt::CLOSED | rt::REVOKED)
                )
            {
                return Err(e);
            }
        }
        if let Some(mut g) = grant {
            let result = g.serve(&mut s, client);
            // L'admission a exigé un écrivain au repos. Seul le fils actif
            // a pu préparer des octets pendant cette délégation.
            s.pending = None;
            if let Some((parent, name, id)) = g.abandoned_copy()
                && s.failed.is_none()
            {
                let fs = s.fs.as_mut().ok_or(rt::CLOSED)?;
                if let Err(e) = selected::discard(fs, parent, &name, id) {
                    let status = namespace::error(e);
                    if matches!(status, w::UNCERTAIN | w::IO | w::CORRUPT) {
                        s.failed = Some(status);
                    }
                    result?;
                    return Err(status);
                }
            }
            result?;
        }
        if done && other.is_none_or(|c| c.closed) {
            return Ok(());
        }
    }
}

#[cfg(feature = "installation")]
fn identifiants_installation() -> rt::Result<novafs::Ids> {
    let mut lv = [0; 16];
    lv[..8].copy_from_slice(&rt::config(2).to_le_bytes());
    lv[8..].copy_from_slice(&rt::config(3).to_le_bytes());
    if lv == [0; 16] {
        return Err(rt::BAD_ARGUMENT);
    }
    let mut fs_id = lv;
    fs_id[15] ^= 1;
    Ok(novafs::Ids { fs: fs_id, lv })
}

#[cfg(feature = "installation")]
pub(crate) fn classer(dev: &IpcDevice<'_>) -> rt::Result<()> {
    let ids = identifiants_installation()?;
    let inspecter = || -> novafs::Result<bool> {
        let fs = Engine::mount_current(dev, ids, Mode::ReadOnly)?;
        fs.verify()?;
        if fs.atime_policy() != Some(novafs::time::Atime::Relative) {
            return Ok(false);
        }
        match fs.read("/systeme", 0, 64) {
            Ok(contenu) => Ok(contenu == b"NovaOS\n"),
            Err(novafs::Error::NotFound | novafs::Error::IsDirectory) => Ok(false),
            Err(e) => Err(e),
        }
    };
    let msg: &[u8] = match inspecter() {
        Ok(true) => b"DISQUE installation existante",
        Ok(false) => b"DISQUE non vierge origine indeterminee",
        Err(novafs::Error::Corrupt(_)) => b"DISQUE metadonnees incoherentes",
        Err(_) => b"DISQUE indetermine lecture P9",
    };
    rt::value(rt::call(
        rt::DEBUG_WRITE,
        msg.as_ptr() as u64,
        msg.len() as u64,
        0,
    ))?;
    Ok(())
}

#[cfg(feature = "installation")]
pub(crate) fn initialiser(dev: &IpcDevice<'_>) -> rt::Result<()> {
    use novafs::time::Atime;
    let ids = identifiants_installation()?;
    let clock = Arc::new(Civil(AtomicI64::new(0)));
    let utc = clock.sample()?;
    let mut fs = Engine::format_p9(dev, ids, Atime::Relative, Some(clock)).map_err(error)?;
    let contenu = b"NovaOS\n";
    let script = include_bytes!("verification.nsh");
    if fs.times("/").map_err(error)?.btime != utc {
        return Err(w::CORRUPT);
    }
    fs.create("/systeme").map_err(error)?;
    fs.write("/systeme", 0, contenu).map_err(error)?;
    fs.create("/verification.nsh").map_err(error)?;
    fs.write("/verification.nsh", 0, script).map_err(error)?;
    let mut disk = fs.unmount().map_err(error)?;
    disk.flush().map_err(error)?;
    let fs = Engine::mount_current(disk, ids, Mode::ReadOnly).map_err(error)?;
    fs.verify().map_err(error)?;
    if fs.atime_policy() != Some(Atime::Relative)
        || fs.read("/systeme", 0, 64).map_err(error)? != contenu
        || fs.read("/verification.nsh", 0, script.len() + 1).map_err(error)? != script
        || fs.times("/systeme").map_err(error)?.btime != utc
    {
        return Err(w::CORRUPT);
    }
    fs.unmount().map_err(error)?;
    Ok(())
}
