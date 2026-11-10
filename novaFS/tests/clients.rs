//! V01–V06. Contrôleur de scénarios macOS : les deux ports sont attribués par
//! le banc. Il n'y a ici ni processus NovaOS, ni nouveau syscall ni service.
use novafs::{Engine, Error, Ids, Kind, Result, device::SimDevice};
use std::{
    collections::VecDeque,
    sync::{Arc, Barrier, Mutex},
    thread,
};

const IDS: Ids = Ids {
    fs: [41; 16],
    lv: [62; 16],
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Port {
    A,
    B,
}
impl Port {
    fn i(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }
}
#[derive(Clone)]
struct Open {
    key: u64,
    path: String,
    oid: u64,
    pos: u64,
    write: bool,
}
struct Session {
    root: &'static str,
    next: u64,
    slots: Vec<Open>,
    incarnation: u64,
}
struct Lab {
    fs: Engine<SimDevice>,
    sessions: [Session; 2],
    next_incarnation: u64,
}
impl Lab {
    fn new() -> Self {
        let mut fs = Engine::format(SimDevice::new(1024, 512).unwrap(), IDS).unwrap();
        fs.mkdir("/a").unwrap();
        fs.mkdir("/b").unwrap();
        let sessions = [("/a", 1), ("/b", 2)].map(|(root, incarnation)| Session {
            root,
            next: 1,
            slots: Vec::new(),
            incarnation,
        });
        Self {
            fs,
            sessions,
            next_incarnation: 3,
        }
    }
    /// Résolution par composant avec racine privée. Une vue volontairement
    /// partagée en lecture seule sert au test de conflit entre deux montages.
    fn resolve(&self, q: Port, name: &str, create: bool) -> Result<(String, bool)> {
        if name.is_empty() || name.len() > 1024 || name.as_bytes().contains(&0) {
            return Err(Error::Invalid("chemin"));
        }
        let root = self.sessions[q.i()].root;
        let mut path = root.to_owned();
        let mut ro = false;
        if name == "/" {
            return Ok((path, ro));
        }
        let mut parts = name.strip_prefix('/').unwrap_or(name).split('/').peekable();
        let mut n = 0;
        while let Some(part) = parts.next() {
            n += 1;
            if n > 32 || part.is_empty() || part.len() > 255 {
                return Err(Error::Invalid("composant"));
            }
            if self.fs.stat(&path)?.kind != Kind::Directory {
                return Err(Error::NotDirectory);
            }
            if create && parts.peek().is_none() && matches!(part, "." | "..") {
                return Err(Error::Invalid("nom réservé"));
            }
            match part {
                "." => {}
                ".." => {
                    if ro && path == "/a" {
                        ro = false;
                        path = root.to_owned()
                    } else if path != root {
                        path.truncate(path.rfind('/').unwrap());
                    }
                }
                "partage" if matches!(q, Port::B) && path == root => {
                    path = "/a".to_owned();
                    ro = true;
                }
                _ => {
                    path.push('/');
                    path.push_str(part);
                    if !create || parts.peek().is_some() {
                        self.fs.stat(&path)?;
                    }
                }
            }
        }
        Ok((path, ro))
    }
    fn open(&mut self, q: Port, name: &str, create: bool, write: bool) -> Result<u64> {
        let (path, ro) = self.resolve(q, name, create)?;
        if ro && (write || create) {
            return Err(Error::ReadOnly);
        }
        let s = &mut self.sessions[q.i()];
        if s.slots.len() >= 32 {
            return Err(Error::Limit("ouvertures"));
        }
        if s.next > u32::MAX as u64 {
            return Err(Error::Limit("handles"));
        }
        let key = (s.incarnation << 32) | s.next;
        s.next += 1;
        if create {
            self.fs.create(&path)?;
        }
        let st = self.fs.stat(&path)?;
        s.slots.push(Open {
            key,
            path,
            oid: st.id,
            pos: 0,
            write,
        });
        Ok(key)
    }
    fn open_bytes(&mut self, q: Port, name: &[u8], create: bool, write: bool) -> Result<u64> {
        if name.len() > 1024 {
            return Err(Error::Invalid("chemin trop long"));
        }
        let path = std::str::from_utf8(name).map_err(|_| Error::Invalid("nom UTF-8"))?;
        self.open(q, path, create, write)
    }
    fn handle(&self, q: Port, h: u64) -> Result<&Open> {
        self.sessions[q.i()]
            .slots
            .iter()
            .find(|o| o.key == h)
            .ok_or(Error::Invalid("autorité locale"))
    }
    fn offset(&mut self, q: Port, h: u64, v: u64) {
        self.sessions[q.i()]
            .slots
            .iter_mut()
            .find(|o| o.key == h)
            .unwrap()
            .pos = v;
    }
    fn seek(&mut self, q: Port, h: u64, delta: i64, origin: u8) -> Result<u64> {
        let o = self.handle(q, h)?;
        let st = self.fs.stat(&o.path)?;
        if st.kind != Kind::File {
            return Err(Error::IsDirectory);
        }
        let base = match origin {
            0 => 0,
            1 => o.pos,
            2 => st.size,
            _ => return Err(Error::Invalid("origine")),
        };
        let pos = base
            .checked_add_signed(delta)
            .filter(|v| *v <= i64::MAX as u64)
            .ok_or(Error::Invalid("déplacement"))?;
        self.offset(q, h, pos);
        Ok(pos)
    }
    fn write(&mut self, q: Port, h: u64, data: &[u8]) -> Result<()> {
        let o = self.handle(q, h)?.clone();
        if !o.write {
            return Err(Error::ReadOnly);
        }
        if data.len() > 4096 {
            return Err(Error::Limit("unité client"));
        }
        self.fs.write(&o.path, o.pos, data)?;
        self.offset(q, h, o.pos + data.len() as u64);
        Ok(())
    }
    fn read(&mut self, q: Port, h: u64, n: usize) -> Result<Vec<u8>> {
        if n > 4096 {
            return Err(Error::Limit("unité client"));
        }
        let o = self.handle(q, h)?.clone();
        let data = self.fs.read(&o.path, o.pos, n)?;
        self.offset(q, h, o.pos + data.len() as u64);
        Ok(data)
    }
    fn close(&mut self, q: Port, h: u64) -> Result<()> {
        let slots = &mut self.sessions[q.i()].slots;
        let i = slots
            .iter()
            .position(|o| o.key == h)
            .ok_or(Error::Invalid("handle fermé"))?;
        slots.remove(i);
        Ok(())
    }
    fn unlink(&mut self, q: Port, name: &str) -> Result<()> {
        let (path, ro) = self.resolve(q, name, false)?;
        if ro {
            return Err(Error::ReadOnly);
        }
        let oid = self.fs.stat(&path)?.id;
        if self
            .sessions
            .iter()
            .flat_map(|s| &s.slots)
            .any(|o| o.oid == oid)
        {
            return Err(Error::Busy);
        }
        self.fs.unlink(&path)
    }
    fn move_entry(&mut self, from: &str, to: &str) -> Result<()> {
        let is_inside = |path: &str, dir: &str| {
            path == dir || path.strip_prefix(dir).is_some_and(|s| s.starts_with('/'))
        };
        for s in &self.sessions {
            if is_inside(s.root, from) {
                return Err(Error::Busy);
            }
            for o in &s.slots {
                if is_inside(&o.path, from)
                    && !is_inside(
                        to,
                        if is_inside(&o.path, s.root) {
                            s.root
                        } else {
                            "/a"
                        },
                    )
                {
                    return Err(Error::Busy);
                }
            }
        }
        self.fs.rename(from, to)?;
        for s in &mut self.sessions {
            for o in &mut s.slots {
                if is_inside(&o.path, from) {
                    o.path.replace_range(..from.len(), to);
                }
            }
        }
        Ok(())
    }
    fn rename(&mut self, q: Port, src: &str, dst: &str) -> Result<()> {
        let (a, ra) = self.resolve(q, src, false)?;
        let (b, rb) = self.resolve(q, dst, true)?;
        if ra != rb {
            return Err(Error::Invalid("montages distincts"));
        }
        if ra {
            return Err(Error::ReadOnly);
        }
        self.move_entry(&a, &b)
    }
    fn revoke(&mut self, q: Port) {
        let s = &mut self.sessions[q.i()];
        s.incarnation = self.next_incarnation;
        self.next_incarnation += 1;
        s.next = 1;
        s.slots.clear();
    }
}

#[test]
fn v01_objets_positions_reutilisation() {
    let mut l = Lab::new();
    let a = Port::A;
    let w = l.open(a, "f", true, true).unwrap();
    let r = l.open(a, "f", false, false).unwrap();
    l.seek(a, w, 4094, 0).unwrap();
    l.write(a, w, b"abcde").unwrap();
    assert_eq!(l.read(a, r, 4095).unwrap().last(), Some(&b'a'));
    assert_eq!(l.read(a, r, 9).unwrap(), b"bcde");
    assert!(l.read(a, r, 1).unwrap().is_empty());
    assert!(l.seek(a, r, i64::MIN, 1).is_err());
    assert_eq!(l.seek(a, r, -2, 2).unwrap(), 4097);
    assert_eq!(l.read(a, r, 2).unwrap(), b"de");
    l.close(a, r).unwrap();
    assert!(l.unlink(a, "f").is_err());
    l.close(a, w).unwrap();
    l.unlink(a, "f").unwrap();
    let next = l.open(a, "f", true, true).unwrap();
    assert_ne!(w, next);
    assert!(l.write(a, w, b"x").is_err());
    assert_eq!(l.fs.stat("/a/f").unwrap().size, 0);
    l.fs.verify().unwrap();
}

#[test]
fn v02_chemins_et_frontieres() {
    let mut l = Lab::new();
    let a = Port::A;
    let long = "x".repeat(255);
    l.open(a, &long, true, false).unwrap();
    assert!(l.open(a, &"x".repeat(256), true, false).is_err());
    for name in ["é", "e\u{301}"] {
        l.open(a, name, true, false).unwrap();
    }
    for bad in ["bad\0name", "//x", "x/", "", "absent/../é"] {
        assert!(l.open(a, bad, false, false).is_err())
    }
    let before = l.fs.receipt();
    assert!(matches!(
        l.open_bytes(a, b"f\xc3\x28", true, true),
        Err(Error::Invalid("nom UTF-8"))
    ));
    assert_eq!(l.fs.receipt(), before);
    l.open_bytes(a, "algèbre".as_bytes(), true, false).unwrap();
    assert_eq!(l.resolve(a, "/../../é", false).unwrap().0, "/a/é");
    // Test des limites du namespace moteur à sa véritable racine.
    let mut p = String::new();
    for _ in 0..32 {
        p.push_str("/d");
        l.fs.mkdir(&p).unwrap();
    }
    assert!(l.fs.mkdir(&(p + "/e")).is_err());
    let mut path = String::new();
    for byte in ['j', 'k', 'l', 'm'] {
        path.push('/');
        path.push_str(&byte.to_string().repeat(255));
        l.fs.mkdir(&path).unwrap();
    }
    assert_eq!(path.len(), 1024);
    assert!(l.fs.stat(&(path + "x")).is_err());
    l.fs.verify().unwrap();
}
#[test]
fn v03_mutations_et_conflits() {
    let l = Arc::new(Mutex::new(Lab::new()));
    let start = Arc::new(Barrier::new(2));
    let wins = thread::scope(|s| {
        let tasks: Vec<_> = (0..2)
            .map(|_| {
                let l = l.clone();
                let start = start.clone();
                s.spawn(move || {
                    start.wait();
                    l.lock().unwrap().fs.mkdir("/a/concurrent").is_ok()
                })
            })
            .collect();
        tasks
            .into_iter()
            .map(|t| usize::from(t.join().unwrap()))
            .sum::<usize>()
    });
    assert_eq!(wins, 1);
    let mut l = l.lock().unwrap();
    l.fs.mkdir("/a/autre").unwrap();
    let h = l.open(Port::A, "concurrent/source", true, true).unwrap();
    l.write(Port::A, h, b"secret").unwrap();
    l.rename(Port::A, "concurrent/source", "autre/cible")
        .unwrap();
    assert!(matches!(
        l.rename(Port::A, "autre/cible", "concurrent"),
        Err(Error::Exists)
    ));
    assert!(l.unlink(Port::A, "autre").is_err());
    assert!(l.fs.rename("/a", "/a/autre/a").is_err());
    assert!(l.rename(Port::B, "partage/autre/cible", "x").is_err());
    assert_eq!(l.fs.read("/a/autre/cible", 0, 6).unwrap(), b"secret");
    l.fs.verify().unwrap();
}
#[test]
fn v04_autorite_locale_et_quotas() {
    let mut l = Lab::new();
    l.open(Port::A, "secret", true, true).unwrap();
    let h = l.open(Port::A, "secret", false, true).unwrap();
    let before = l.fs.receipt();
    assert!(l.write(Port::B, h, b"intrus").is_err());
    for candidate in ["../../a/secret", "/dev/disk0", "/volume/brut"] {
        assert!(l.open(Port::B, candidate, false, true).is_err());
    }
    assert!(l.open(Port::B, "partage/secret", false, true).is_err());
    assert_eq!(l.fs.receipt(), before);
    for _ in 2..32 {
        l.open(Port::A, "secret", false, false).unwrap();
    }
    assert!(matches!(
        l.open(Port::A, "interdit", true, true),
        Err(Error::Limit(_))
    ));
    assert!(l.fs.stat("/a/interdit").is_err());
    l.close(Port::A, h).unwrap();
    l.open(Port::A, "repris", true, true).unwrap();
}

// Assemblage d'une unique écriture à partir de vraies trames binaires de 128 octets.
// Les queues et le reçu appartiennent au banc, hors ABI noyau.
struct Wire {
    port: Port,
    seq: u64,
    session: u64,
    body: Vec<u8>,
    total: Option<usize>,
    receipt: Option<novafs::Ref>,
    queue: VecDeque<Vec<u8>>,
    answers: VecDeque<(u64, novafs::Ref)>,
    pending_reply: bool,
    emitted: bool,
    broken: bool,
}
impl Wire {
    fn new(l: &Lab, q: Port) -> Self {
        Self {
            port: q,
            seq: 1,
            session: l.sessions[q.i()].incarnation,
            body: Vec::new(),
            total: None,
            receipt: None,
            queue: VecDeque::new(),
            answers: VecDeque::new(),
            pending_reply: false,
            emitted: false,
            broken: false,
        }
    }
    fn bound(&self, l: &Lab, q: Port) -> Result<()> {
        if self.broken || self.port != q || self.session != l.sessions[q.i()].incarnation {
            return Err(Error::Invalid("canal étranger ou périmé"));
        }
        Ok(())
    }
    fn enqueue(&mut self, b: Vec<u8>) -> Result<()> {
        if self.broken {
            return Err(Error::Invalid("canal fermé"));
        }
        if self.queue.len() == 8 {
            return Err(Error::Busy);
        }
        if b.len() > 128 {
            return Err(Error::Invalid("trame"));
        }
        self.queue.push_back(b);
        Ok(())
    }
    fn take(&mut self, l: &mut Lab, q: Port) -> Result<()> {
        self.bound(l, q)?;
        let b = self.queue.pop_front().ok_or(Error::Busy)?;
        let r = self.dispatch(l, &b);
        if matches!(r, Err(Error::Invalid(_))) {
            self.broken = true;
            self.queue.clear();
            self.answers.clear();
            self.body.clear();
        }
        r
    }
    fn dispatch(&mut self, l: &mut Lab, b: &[u8]) -> Result<()> {
        if b.len() < 32
            || b[0] != 1
            || b[2] != 1
            || b[3] != 0
            || b[4..8] != [0; 4]
            || b[26..32] != [0; 6]
        {
            return Err(Error::Invalid("en-tête"));
        }
        let seq = u64::from_le_bytes(b[8..16].try_into().unwrap());
        if b[1] == 3 || b[1] == 5 {
            if b.len() != 32 || b[16..].iter().any(|&v| v != 0) || seq == 0 || seq > self.seq {
                return Err(Error::Invalid("contrôle"));
            }
            if seq < self.seq {
                return Ok(());
            }
            if b[1] == 3 {
                if self.receipt.is_some() {
                    self.pending_reply = true;
                }
            } else {
                if !self.emitted {
                    return Err(Error::Invalid("acquittement anticipé"));
                }
                self.receipt = None;
                self.body.clear();
                self.total = None;
                self.emitted = false;
                self.pending_reply = false;
                self.seq = self
                    .seq
                    .checked_add(1)
                    .ok_or(Error::Limit("séquence épuisée"))?;
            }
            return Ok(());
        }
        let total = u32::from_le_bytes(b[16..20].try_into().unwrap()) as usize;
        let off = u32::from_le_bytes(b[20..24].try_into().unwrap()) as usize;
        let len = u16::from_le_bytes(b[24..26].try_into().unwrap()) as usize;
        if b[1] != 1
            || self.receipt.is_some()
            || self.seq != seq
            || !(16..=4160).contains(&total)
            || self.total.is_some_and(|t| t != total)
            || off != self.body.len()
            || len != (total.saturating_sub(off)).min(96)
            || b.len() != 32 + len
        {
            return Err(Error::Invalid("fragment"));
        }
        self.total = Some(total);
        self.body.extend_from_slice(&b[32..]);
        if self.body.len() == total {
            let h = u64::from_le_bytes(self.body[..8].try_into().unwrap());
            let count = u32::from_le_bytes(self.body[8..12].try_into().unwrap()) as usize;
            if count != total - 16 || self.body[12..16] != [0; 4] {
                return Err(Error::Invalid("corps write"));
            }
            l.write(self.port, h, &self.body[16..])?;
            self.receipt = Some(l.fs.receipt());
            self.pending_reply = true;
        }
        Ok(())
    }
    fn emit_reply(&mut self) -> Result<()> {
        if !self.pending_reply || self.answers.len() == 8 {
            return Err(Error::Busy);
        }
        self.answers
            .push_back((self.seq, self.receipt.ok_or(Error::Busy)?));
        self.pending_reply = false;
        self.emitted = true;
        Ok(())
    }
    fn replay(&self, l: &Lab, q: Port, seq: u64) -> Result<novafs::Ref> {
        self.bound(l, q)?;
        if seq != self.seq {
            return Err(Error::Invalid("reçu périmé"));
        }
        self.receipt.ok_or(Error::Busy)
    }
}
fn control(kind: u8, seq: u64) -> Vec<u8> {
    let mut frame = vec![0; 32];
    frame[..3].copy_from_slice(&[1, kind, 1]);
    frame[8..16].copy_from_slice(&seq.to_le_bytes());
    frame
}
fn frames_for(seq: u64, h: u64, data: &[u8]) -> Vec<Vec<u8>> {
    assert!(data.len() <= 4096);
    let mut body = Vec::from(h.to_le_bytes());
    body.extend_from_slice(&(data.len() as u32).to_le_bytes());
    body.extend_from_slice(&[0; 4]);
    body.extend_from_slice(data);
    body.chunks(96)
        .enumerate()
        .map(|(i, p)| {
            let mut b = control(1, seq);
            b[16..20].copy_from_slice(&(body.len() as u32).to_le_bytes());
            b[20..24].copy_from_slice(&(i as u32 * 96).to_le_bytes());
            b[24] = p.len() as u8;
            b.extend_from_slice(p);
            b
        })
        .collect()
}
fn frames(h: u64) -> Vec<Vec<u8>> {
    frames_for(1, h, &(0..4096).map(|i| i as u8).collect::<Vec<_>>())
}
#[test]
fn v05_fragments_rejeu_et_mort() {
    let mut l = Lab::new();
    let q = Port::A;
    let h = l.open(q, "cible", true, true).unwrap();
    let parts = frames(h);
    assert_eq!(parts.len(), 43);
    for fault in 0..7 {
        let mut w = Wire::new(&l, q);
        let initial = l.fs.receipt();
        w.enqueue(parts[0].clone()).unwrap();
        w.take(&mut l, q).unwrap();
        let mut bad = parts[1].clone();
        match fault {
            0 => bad = parts[0].clone(),
            1 => bad[20] = 97,
            2 => bad[16..20].fill(255),
            3 => {
                bad.pop();
            }
            4 => bad[0] = 2,
            5 => bad[2] = 0xff,
            _ => bad[26] = 1,
        }
        w.enqueue(bad).unwrap();
        assert!(w.take(&mut l, q).is_err());
        assert_eq!(initial, l.fs.receipt());
    }
    let mut abandoned = Wire::new(&l, q);
    abandoned.enqueue(parts[0].clone()).unwrap();
    l.revoke(q);
    assert!(abandoned.take(&mut l, q).is_err());
    let new_h = l.open(q, "cible", false, true).unwrap();
    let parts = frames(new_h);
    let mut w = Wire::new(&l, q);
    for f in &parts[..8] {
        w.enqueue(f.clone()).unwrap();
    }
    assert!(matches!(w.enqueue(parts[8].clone()), Err(Error::Busy)));
    let b = l.open(Port::B, "autre", true, true).unwrap();
    l.write(Port::B, b, b"ok").unwrap();
    for _ in 0..8 {
        w.take(&mut l, q).unwrap();
    }
    for f in &parts[8..42] {
        w.enqueue(f.clone()).unwrap();
        w.take(&mut l, q).unwrap();
    }
    assert_eq!(l.fs.stat("/a/cible").unwrap().size, 0);
    w.enqueue(parts[42].clone()).unwrap();
    w.take(&mut l, q).unwrap();
    let confirmed = w.replay(&l, q, 1).unwrap();
    assert_eq!(w.replay(&l, q, 1).unwrap(), confirmed);
    assert_eq!(l.handle(q, new_h).unwrap().pos, 4096);
    l.revoke(q);
    assert!(w.replay(&l, q, 1).is_err());
    assert_eq!(l.fs.receipt(), confirmed);
    assert_eq!(
        l.fs.read("/a/cible", 0, 4096).unwrap(),
        (0..4096).map(|i| i as u8).collect::<Vec<_>>()
    );
}
#[test]
fn v06_concurrence_et_vues() {
    let mut l = Lab::new();
    l.fs.mkdir("/a/d").unwrap();
    let h = l.open(Port::A, "d/f", true, true).unwrap();
    let alias = l.open(Port::B, "partage/d/f", false, false).unwrap();
    assert!(matches!(l.move_entry("/a/d", "/b/d"), Err(Error::Busy)));
    assert!(matches!(l.move_entry("/a/d/f", "/b/f"), Err(Error::Busy)));
    assert!(l.unlink(Port::A, "d/f").is_err());
    l.close(Port::A, h).unwrap();
    assert!(l.unlink(Port::A, "d/f").is_err());
    l.close(Port::B, alias).unwrap();
    let lab = Arc::new(Mutex::new(l));
    let barrier = Arc::new(Barrier::new(3));
    thread::scope(|s| {
        for val in [7, 9] {
            let lab = lab.clone();
            let barrier = barrier.clone();
            s.spawn(move || {
                barrier.wait();
                for _ in 0..20 {
                    lab.lock()
                        .unwrap()
                        .fs
                        .write("/a/d/f", 0, &[val; 4096])
                        .unwrap();
                }
            });
        }
        barrier.wait();
        for _ in 0..40 {
            let observed = lab.lock().unwrap().fs.read("/a/d/f", 0, 4096).unwrap();
            thread::yield_now();
            assert!(observed.is_empty() || observed == vec![7; 4096] || observed == vec![9; 4096]);
        }
    });
    let mut l = lab.lock().unwrap();
    let old = l.fs.read("/a/d/f", 0, 4096).unwrap();
    for _ in 0..4 {
        l.fs.write("/a/d/f", 0, &[1; 4096]).unwrap();
    }
    assert!(old == vec![7; 4096] || old == vec![9; 4096]);
    l.fs.verify().unwrap();
}

#[test]
fn v04_offre_initiale_silencieuse_et_rendez_vous_du_second_client() {
    let mut lab = Lab::new();
    // Le modèle de progression livre des ticks discrets, indépendants du temps hôte.
    // Les deux liaisons privées sont provisionnées avant le premier essai de don.
    let mut bindings = [
        Some(Wire::new(&lab, Port::A)),
        Some(Wire::new(&lab, Port::B)),
    ];
    let mut accepted = [false; 2];
    let mut attempts = [0; 2];
    let mut ticks = 0;
    let mut b_active_at = None;
    for _round in 0..8 {
        for port in [Port::A, Port::B] {
            let i = port.i();
            if accepted[i] {
                continue;
            }
            attempts[i] += 1;
            for _ in 0..8 {
                ticks += 1;
                // B est en réception sur sa R depuis le tick 3. A ignore sa R.
                if port == Port::B && ticks >= 3 {
                    accepted[i] = true;
                    break;
                }
            }
            if accepted[i] {
                let h = lab.open(port, "servi", true, true).unwrap();
                let w = bindings[i].as_mut().unwrap();
                w.enqueue(frames_for(1, h, b"B").pop().unwrap()).unwrap();
                w.take(&mut lab, port).unwrap();
                w.emit_reply().unwrap();
                assert!(w.answers.pop_front().is_some());
                b_active_at = Some(ticks);
            } else if attempts[i] == 8 {
                lab.revoke(port);
                bindings[i] = None;
            }
        }
    }
    assert_eq!(attempts, [8, 1]);
    assert_eq!(b_active_at, Some(9));
    assert!(bindings[0].is_none() && bindings[1].is_some());
    assert_eq!(lab.fs.read("/b/servi", 0, 1).unwrap(), b"B");
}

#[test]
fn v05_r_pleine_ack_et_fragments_retardes() {
    let mut lab = Lab::new();
    let h = lab.open(Port::A, "cible", true, true).unwrap();
    let mut a = Wire::new(&lab, Port::A);
    let b_handle = lab.open(Port::B, "autre", true, true).unwrap();
    let mut b = Wire::new(&lab, Port::B);
    let pending = frames_for(1, h, b"premier").pop().unwrap();
    a.enqueue(pending).unwrap();
    // Le même objet du banc ne peut jamais être consommé depuis le port voisin.
    assert!(a.take(&mut lab, Port::B).is_err());
    assert_eq!(a.queue.len(), 1);
    a.take(&mut lab, Port::A).unwrap();
    a.emit_reply().unwrap();
    let (seq, first) = a.answers.pop_front().unwrap();
    assert_eq!(seq, 1);
    for _ in 0..8 {
        a.enqueue(control(3, 1)).unwrap();
        a.take(&mut lab, Port::A).unwrap();
        a.emit_reply().unwrap();
    }
    a.enqueue(control(3, 1)).unwrap();
    a.take(&mut lab, Port::A).unwrap();
    assert!(matches!(a.emit_reply(), Err(Error::Busy)));
    assert_eq!(a.answers.len(), 8);
    assert_eq!(lab.handle(Port::A, h).unwrap().pos, 7);
    assert_eq!(lab.fs.receipt(), first);
    b.enqueue(frames_for(1, b_handle, b"libre").pop().unwrap())
        .unwrap();
    b.take(&mut lab, Port::B).unwrap();
    b.emit_reply().unwrap();
    assert_eq!(b.answers.pop_front().unwrap().0, 1);
    a.enqueue(control(5, 1)).unwrap();
    a.take(&mut lab, Port::A).unwrap();
    assert_eq!(a.answers.len(), 8);
    assert!(!a.pending_reply);
    // N+1 est admise sur Q après ACK, alors que les copies de N sont dans R.
    a.enqueue(frames_for(2, h, b" suite").pop().unwrap())
        .unwrap();
    a.take(&mut lab, Port::A).unwrap();
    assert!(matches!(a.emit_reply(), Err(Error::Busy)));
    let mut retired = 0;
    let mut current = None;
    while let Some((number, receipt)) = a.answers.pop_front() {
        if number < 2 {
            assert_eq!(receipt, first);
            retired += 1;
        } else {
            current = Some(receipt);
        }
        if a.pending_reply {
            a.emit_reply().unwrap();
        }
    }
    assert_eq!(retired, 8);
    assert_eq!(current, Some(lab.fs.receipt()));
    assert_eq!(lab.handle(Port::A, h).unwrap().pos, 13);
    assert_eq!(lab.fs.read("/a/cible", 0, 20).unwrap(), b"premier suite");
    a.enqueue(control(5, 2)).unwrap();
    a.take(&mut lab, Port::A).unwrap();
    a.enqueue(control(3, 1)).unwrap();
    a.take(&mut lab, Port::A).unwrap();
    assert!(!a.pending_reply);
    assert!(a.receipt.is_none());
}

#[test]
fn v06_ouverture_par_alias_contre_unlink() {
    // Deux ordres forcés et un départ simultané : le mutex commun de l'instance
    // couvre le contrôle de toutes ses ouvertures et la mutation du catalogue.
    for order in 0..3 {
        let mut state = Lab::new();
        state.fs.create("/a/source").unwrap();
        let state = Arc::new(Mutex::new(state));
        let barrier = Barrier::new(2);
        let (opened, opening) = std::sync::mpsc::channel();
        let (removed, removal) = std::sync::mpsc::channel();
        let (handle, unlink) = thread::scope(|scope| {
            let barrier = &barrier;
            let left = state.clone();
            let op = scope.spawn(move || {
                barrier.wait();
                if order == 1 {
                    removal.recv().unwrap();
                }
                let result = left
                    .lock()
                    .unwrap()
                    .open(Port::B, "partage/source", false, false);
                if order == 0 {
                    opened.send(()).unwrap();
                }
                result
            });
            let right = state.clone();
            let rm = scope.spawn(move || {
                barrier.wait();
                if order == 0 {
                    opening.recv().unwrap();
                }
                let result = right.lock().unwrap().unlink(Port::A, "source");
                if order == 1 {
                    removed.send(()).unwrap();
                }
                result
            });
            (op.join().unwrap(), rm.join().unwrap())
        });
        match (handle, unlink) {
            (Ok(h), Err(Error::Busy)) => {
                assert_ne!(order, 1);
                let mut lab = state.lock().unwrap();
                assert_eq!(lab.read(Port::B, h, 1).unwrap(), b"");
            }
            (Err(Error::NotFound), Ok(())) => {
                assert_ne!(order, 0);
                assert!(matches!(
                    state.lock().unwrap().fs.stat("/a/source"),
                    Err(Error::NotFound)
                ));
            }
            other => panic!("ordre incompatible {order} : {other:?}"),
        }
        state.lock().unwrap().fs.verify().unwrap();
    }
}
