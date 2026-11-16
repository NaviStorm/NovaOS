//! Profil isolé. Trois extents de fichiers hôte distincts sont présentés sur le
//! seul bail bloc existant. Le moteur P9 ne peut adresser que le premier.
use crate::{device::IpcDevice, runtime as rt};
use alloc::format;
use core::cell::Cell;
use novafs::{Block, Device, Engine, Ids, Mode};
use novaos_identity::{lab::Ledger, *};
static PEAK: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
pub(crate) fn sample_heap() {
    let h = rt::call(rt::HEAP_RESIZE, 0, 0, 0);
    if h.status == rt::OK {
        PEAK.fetch_max(h.value, core::sync::atomic::Ordering::Relaxed);
    }
}
const PAGES: u64 = 512;
const AUTH: u64 = 12;
const DOMAIN: Id = [7; 16];
const IDS: Ids = Ids {
    fs: [8; 16],
    lv: [9; 16],
};
fn marker(s: &str) {
    for bytes in s.as_bytes().chunks(90) {
        rt::call(
            rt::DEBUG_WRITE,
            bytes.as_ptr() as u64,
            bytes.len() as u64,
            0,
        );
    }
}
fn cut(label: &str) -> ! {
    marker(label);
    loop {
        rt::call(rt::THREAD_YIELD, 0, 0, 0);
    }
}
struct Span<'a, D> {
    dev: D,
    start: u64,
    len: u64,
    cut_after: &'a Cell<u32>,
    unavailable: bool,
}
impl<D: Device> Span<'_, D> {
    fn block(&self, n: u64) -> novafs::Result<u64> {
        if n >= self.len {
            Err(novafs::Error::Invalid("hors extent de test"))
        } else {
            Ok(self.start + n)
        }
    }
}
impl<D: Device> Device for Span<'_, D> {
    fn blocks(&self) -> u64 {
        self.len
    }
    fn read(&self, n: u64) -> novafs::Result<Block> {
        if self.unavailable {
            return Err(novafs::Error::Unavailable);
        }
        self.dev.read(self.block(n)?)
    }
    fn write(&mut self, n: u64, data: &Block) -> novafs::Result<()> {
        let n = self.block(n)?;
        self.dev.write(n, data)
    }
    fn flush(&mut self) -> novafs::Result<()> {
        self.dev.flush()?;
        let n = self.cut_after.get();
        if n > 0 {
            self.cut_after.set(n - 1);
            if n == 1 {
                cut("NA024 CUT durable");
            }
        }
        Ok(())
    }
}
fn check(ok: bool) -> Result<()> {
    if ok { Ok(()) } else { Err(Fault::BadInput) }
}
fn suite(dev: &IpcDevice<'_>) -> Result<()> {
    if dev.blocks() != PAGES + AUTH + 1 {
        return Err(Fault::WrongDomain);
    }
    let control = dev.read(PAGES + AUTH)?;
    if &control[..8] != b"NA024LAB" {
        return Err(Fault::Denied);
    }
    let op = control[8];
    let pcut = Cell::new(0);
    let acut = Cell::new(0);
    let disk = Span {
        dev,
        start: 0,
        len: PAGES,
        cut_after: &pcut,
        unavailable: false,
    };
    let mut authority = Ledger::new(Span {
        dev,
        start: PAGES,
        len: AUTH,
        cut_after: &acut,
        unavailable: op == 18,
    });
    if op == 0 {
        provision(disk, IDS, DOMAIN, [4; 16], &mut authority)?.unmount()?;
        return Ok(());
    }
    let mut fs = Engine::mount_current(disk, IDS, Mode::ReadWrite)?;
    if op == 10 {
        for p in PATHS {
            fs.unlink(p)?;
        }
        check(Enrollment::from_provisioner(&mut authority).is_err())?;
        return check(Registry::open(&mut fs, &mut authority, DOMAIN).is_err());
    }
    if op == 9 {
        return match Registry::open(&mut fs, &mut authority, DOMAIN) {
            Err(Fault::StateMismatch | Fault::StaleVolume) => Ok(()),
            _ => Err(Fault::BadInput),
        };
    }
    if op == 12 {
        let h = authority.current()?;
        fs.prepare_write_id(h.objects[2], (h.generation % 2) * 16384 + 54, b"X")?
            .commit()?;
        return check(matches!(
            Registry::open(&mut fs, &mut authority, DOMAIN),
            Err(Fault::StateMismatch)
        ));
    }
    if op == 17 {
        return check(matches!(
            Registry::open(&mut fs, &mut authority, [66; 16]),
            Err(Fault::WrongDomain)
        ));
    }
    if op == 18 {
        return check(matches!(
            Registry::open(&mut fs, &mut authority, DOMAIN),
            Err(Fault::Unavailable)
        ));
    }
    if op == 14 || op == 15 {
        let h = authority.current()?;
        let expected = if op == 14 { 0 } else { 1 };
        check(h.generation == expected)?;
        let mut requests = [(0u8, 0u64); 2];
        for (slot, item) in requests.iter_mut().enumerate() {
            let start = 16 + slot * 9;
            *item = (
                control[start],
                u64::from_le_bytes(control[start + 1..start + 9].try_into().unwrap()),
            );
            check(item.0 > 0 && item.1 == expected)?;
        }
        check(requests[0].0 != requests[1].0)?;
        let [(id1, _), (id2, _)] = requests;
        if op == 14 {
            let a = Enrollment::from_provisioner(&mut authority)?;
            let b = Enrollment::from_provisioner(&mut authority)?;
            let mut r = Registry::open(&mut fs, &mut authority, DOMAIN)?;
            let won = r.enroll(&a, [id1; 16], "admin", [2; 16], "users")?;
            check(matches!(
                r.enroll(&b, [id2; 16], "second", [3; 16], "second"),
                Err(Fault::Denied)
            ))?;
            check(r.outcome([id1; 16])? == Some(won) && r.outcome([id2; 16])?.is_none())?;
        } else {
            let mut r = Registry::open(&mut fs, &mut authority, DOMAIN)?;
            let g1 = r.grant_attested(created_user(&h))?;
            let g2 = r.grant_attested(created_user(&h))?;
            let group = created_group(&h);
            let won = r.mutate(
                g1,
                [id1; 16],
                Change::Create {
                    name: "alice",
                    credential: [5; 16],
                    group,
                },
            )?;
            check(matches!(
                r.mutate(
                    g2,
                    [id2; 16],
                    Change::Create {
                        name: "bob",
                        credential: [6; 16],
                        group
                    }
                ),
                Err(Fault::Conflict)
            ))?;
            check(r.outcome([id1; 16])? == Some(won) && r.outcome([id2; 16])?.is_none())?;
        }
        marker(&format!(
            "NA024 REPLIES {id1}=0 {id2}={} END",
            if op == 14 { "DENIED" } else { "CONFLICT" }
        ));
        return Ok(());
    }
    if op == 13 {
        return check(Registry::open(&mut fs, &mut authority, DOMAIN).is_err());
    }
    if op == 1 || op == 2 {
        let a = Enrollment::from_provisioner(&mut authority)?;
        let b = Enrollment::from_provisioner(&mut authority)?;
        let mut reg = Registry::open(&mut fs, &mut authority, DOMAIN)?;
        if op == 1 {
            acut.set(1);
        }
        reg.enroll(&a, [1; 16], "admin", [2; 16], "users")?;
        return check(matches!(
            reg.enroll(&b, [3; 16], "autre", [4; 16], "autres"),
            Err(Fault::Denied)
        ));
    }
    check(Enrollment::from_provisioner(&mut authority).is_err())?;
    let mut reg = Registry::open(&mut fs, &mut authority, DOMAIN)?;
    let genesis = reg.outcome([1; 16])?.ok_or(Fault::Corrupt)?;
    let admin = created_user(&genesis);
    let group = created_group(&genesis);
    let mut grant = reg.grant_attested(admin)?;
    check(reg.inspect(grant, admin)?.admin)?;
    let create = |name| Change::Create {
        name,
        credential: [5; 16],
        group,
    };
    if op == 3 {
        return check(reg.outcome([2; 16])?.is_none());
    }
    if matches!(op, 4 | 5 | 6 | 11 | 16) {
        if op == 4 {
            pcut.set(2);
        } // F2 du premier fichier, deux autres non écrits.
        if op == 16 {
            pcut.set(6);
        }
        if op == 5 {
            acut.set(1);
        } // décision durable, pas de réponse.
        let h = reg.mutate(
            grant,
            [2; 16],
            create(if op == 11 { "eve" } else { "alice" }),
        )?;
        check(reg.outcome([2; 16])? == Some(h))?;
        return check(matches!(
            reg.mutate(grant, [3; 16], create("other")),
            Err(Fault::Conflict)
        ));
    }
    let first = reg.outcome([2; 16])?.ok_or(Fault::Corrupt)?;
    let uid = created_user(&first);
    let alice = reg.resolve_verified_factor([5; 16])?;
    check(alice.subject() == uid)?;
    check(reg.groups(alice)? == [group] && reg.inspect(alice, uid)?.name == "alice")?;
    check(matches!(reg.inspect(alice, admin), Err(Fault::Denied)))?;
    check(matches!(
        reg.mutate(alice, [9; 16], Change::Delete(admin)),
        Err(Fault::Denied)
    ))?;
    if op == 7 {
        return Ok(());
    }
    check(op == 8)?;
    reg.mutate(grant, [3; 16], Change::Rename(uid, "alice-b"))?;
    grant = reg.grant_attested(admin)?;
    check(reg.inspect(grant, uid)?.name == "alice-b")?;
    reg.mutate(grant, [4; 16], Change::RemoveMember(uid, group))?;
    check(reg.groups(reg.grant_attested(uid)?)?.is_empty())?;
    grant = reg.grant_attested(admin)?;
    reg.mutate(grant, [5; 16], Change::Disable(uid))?;
    check(matches!(reg.grant_attested(uid), Err(Fault::Denied)))?;
    grant = reg.grant_attested(admin)?;
    reg.mutate(grant, [6; 16], Change::Delete(uid))?;
    grant = reg.grant_attested(admin)?;
    let newgroup = reg.mutate(grant, [7; 16], Change::AddGroup("other"))?;
    grant = reg.grant_attested(admin)?;
    let re = reg.mutate(
        grant,
        [8; 16],
        Change::Create {
            name: "alice-b",
            credential: [6; 16],
            group: created_group(&newgroup),
        },
    )?;
    let fresh = reg.resolve_verified_factor([6; 16])?;
    check(
        fresh.subject() == created_user(&re)
            && matches!(reg.resolve_verified_factor([5; 16]), Err(Fault::Denied)),
    )?;
    let privileged = reg.grant_attested(admin)?;
    check(matches!(
        reg.mutate(
            privileged,
            [10; 16],
            Change::Create {
                name: "voisin",
                credential: [5; 16],
                group
            }
        ),
        Err(Fault::Conflict)
    ))?;
    check(
        fresh.subject() != uid
            && reg.groups(fresh)? == [created_group(&newgroup)]
            && matches!(reg.inspect(fresh, uid), Err(Fault::Denied))
            && reg.inspect(alice, uid).is_err(),
    )
}
pub(crate) fn run(dev: &IpcDevice<'_>, sector: u32) -> rt::Result<()> {
    let heap = || rt::call(rt::HEAP_RESIZE, 0, 0, 0).value;
    let start = heap();
    PEAK.store(start, core::sync::atomic::Ordering::Relaxed);
    marker("NA024 BEGIN");
    match suite(dev) {
        Ok(()) => marker(&format!(
            "NA024 OK sector={sector} heap_delta={} heap_peak={} END",
            heap().saturating_sub(start),
            PEAK.load(core::sync::atomic::Ordering::Relaxed)
                .saturating_sub(start)
        )),
        Err(e) => marker(&format!("NA024 FAIL {e:?} END")),
    }
    loop {
        rt::call(rt::THREAD_YIELD, 0, 0, 0);
    }
}
