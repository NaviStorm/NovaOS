mod support;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    hint::black_box,
    ptr,
    time::Instant,
};
use support::*;

thread_local! {
    static LIVE:Cell<isize>=const{Cell::new(0)};
    static PEAK:Cell<isize>=const{Cell::new(0)};
    static FAIL:Cell<Option<usize>>=const{Cell::new(None)};
    static ANCHOR:Cell<usize>=const{Cell::new(0)};
    static DEPTH:Cell<usize>=const{Cell::new(0)};
}
struct Meter;
#[global_allocator]
static METER: Meter = Meter;
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let local = 0u8;
        let sp = black_box(&local) as *const u8 as usize;
        ANCHOR.with(|a| {
            if a.get() != 0 {
                DEPTH.with(|d| d.set(d.get().max(a.get().abs_diff(sp))))
            }
        });
        if FAIL.with(|f| match f.get() {
            Some(0) => {
                f.set(None);
                true
            }
            Some(n) => {
                f.set(Some(n - 1));
                false
            }
            None => false,
        }) {
            return ptr::null_mut();
        }
        let p = unsafe { System.alloc(l) };
        if !p.is_null() {
            LIVE.with(|v| {
                let n = v.get() + l.size() as isize;
                v.set(n);
                PEAK.with(|h| h.set(h.get().max(n)));
            });
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.with(|v| v.set(v.get() - l.size() as isize));
        unsafe { System.dealloc(p, l) }
    }
}
fn sample<T>(f: impl FnOnce() -> T) -> (T, isize, isize) {
    let start = LIVE.with(Cell::get);
    PEAK.with(|p| p.set(start));
    let result = f();
    (
        result,
        PEAK.with(Cell::get) - start,
        LIVE.with(Cell::get) - start,
    )
}
fn each<T>(name: &str, mut attempt: impl FnMut() -> novaos_gestionnaire_sim::Result<T>) -> T {
    for n in 0..8192 {
        let before = LIVE.with(Cell::get);
        FAIL.with(|f| f.set(Some(n)));
        let r = attempt();
        let unused = FAIL.with(|f| f.replace(None));
        if unused.is_some() {
            println!("allocations {name} {n}");
            return r.unwrap();
        }
        assert!(r.is_err(), "{name} {n}");
        assert_eq!(LIVE.with(Cell::get), before, "{name} {n}");
    }
    panic!("opération trop longue")
}
#[test]
fn echec_de_chaque_allocation_et_conservation() {
    let p = lab::policy();
    let c = catalog(&p, 1);
    let mut m = each("registre", || {
        Manager::new(1, &c, 1, Use::LAB, Use::default(), None, 0)
    });
    m.refresh(1, true, [node(0)]).unwrap();
    each("inventaire128", || {
        let result = m.refresh(2, true, (0..128).map(node));
        if result.is_err() {
            assert_eq!(m.inventory().revision(), 1);
            assert_eq!(m.inventory().nodes().len(), 1)
        }
        result
    });
    let d = each("groupe", || m.discover(sl(0)));
    let v = each("selection", || {
        let r = checked(&mut m, d);
        if r.is_err() {
            assert_eq!(m.group(sl(0)).unwrap().stamp(), d);
            assert!(m.plan(sl(0)).is_none())
        }
        r
    });
    let r = each("reservation", || {
        let r = m.reserve(v, 16, 1, 3);
        if r.is_err() {
            assert_eq!(m.usage().unwrap(), Use::default());
            assert!(m.plan(sl(0)).is_some())
        }
        r
    });
    let b = Binding {
        owner: 1,
        peer: 2,
        role: Role::Supervisor,
        group: Some(r.attachment),
    };
    let cap = each("liaison", || m.agreed(b));
    let snapshot = m.usage().unwrap();
    let orig = m.management_epoch();
    let links = each("reprise", || {
        let r = m.restart(orig);
        if r.is_err() {
            assert_eq!(m.management_epoch(), orig);
            assert_eq!(m.usage().unwrap(), snapshot)
        }
        r
    });
    assert_ne!(cap.epoch(), links[0].1.epoch());
}
struct Session {
    id: u64,
    catalog: Capability,
    supervisor: Capability,
    launcher: Capability,
    service: Capability,
}
impl Session {
    fn open(m: &mut Manager<'_, '_>, s: Stamp) -> Self {
        let mut pair = |owner, peer, role| {
            m.agreed(Binding {
                owner,
                peer,
                role,
                group: Some(s.attachment),
            })
            .unwrap()
        };
        Self {
            id: 0,
            catalog: pair(2, 1, Role::Catalog),
            supervisor: pair(3, 1, Role::Supervisor),
            launcher: pair(1, 4, Role::Launcher),
            service: pair(1, 5, Role::Service),
        }
    }
    fn call(
        &mut self,
        m: &mut Manager<'_, '_>,
        c: Capability,
        op: Op,
        s: Stamp,
        inc: u64,
    ) -> Stamp {
        self.id += 1;
        let mut r = packet(op, c, self.id, s);
        if op == Op::Reserve {
            r.body.a = 16;
            r.body.b = inc;
            r.body.c = 1
        }
        let label = s.attachment.slot.label();
        let mut t = lab::target(if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "x86_64"
        });
        t.attachment = std::str::from_utf8(&label).unwrap();
        t.revision = m.inventory().revision();
        let mut wire = [0; 128];
        let n = r.encode(&mut wire).unwrap();
        let out = m
            .serve(
                c,
                &wire[..n],
                Some(Selection {
                    target: &t,
                    pin: None,
                    approved_members: 1,
                }),
            )
            .unwrap();
        assert_eq!(out.status, Status::Confirmed, "{op:?}");
        let reply = Reply::decode(r, &out.encode(r)).unwrap();
        self.id += 1;
        let ack = Request {
            op: Op::Ack,
            epoch: c.epoch(),
            id: self.id + 1,
            revision: reply.value,
            body: Body {
                generation: reply.generation,
                a: r.epoch,
                b: r.id,
                c: 0,
            },
        };
        assert_eq!(send(m, c, ack).unwrap(), reply);
        m.group(sl(0)).unwrap().stamp()
    }
    fn cycle(&mut self, m: &mut Manager<'_, '_>, s: Stamp, inc: u64) -> (Stamp, u128) {
        let start = Instant::now();
        let s = self.call(m, self.catalog, Op::Resolve, s, inc);
        let selection = start.elapsed().as_nanos();
        let s = self.call(m, self.supervisor, Op::Reserve, s, inc);
        let s = self.call(m, self.launcher, Op::Prepare, s, inc);
        let s = self.call(m, self.launcher, Op::Launch, s, inc);
        let s = self.call(m, self.supervisor, Op::Assign, s, inc);
        let l = m.group(sl(0)).unwrap().lease().unwrap();
        m.readiness(l, true, false, true).unwrap();
        let s = m.step(s, Event::Initialize).unwrap();
        let s = self.call(m, self.service, Op::PreparePort, s, inc);
        let s = self.call(m, self.service, Op::Publish, s, inc);
        let leaf = m.leaf(l, Right::DmaWrite, "rx").unwrap();
        m.authorize(leaf, 16, inc, Right::DmaWrite, "rx").unwrap();
        let s = self.call(m, self.supervisor, Op::Revoke, s, inc);
        let s = m.step(s, Event::Stop).unwrap();
        let s = self.call(m, self.supervisor, Op::Reconcile, s, inc);
        m.evidence(proof(l, 1, PAGES)).unwrap();
        let s = self.call(m, self.supervisor, Op::Reconcile, s, inc);
        let s = m.step(s, Event::Readmit).unwrap();
        (m.step(s, Event::Rediscover).unwrap(), selection)
    }
}
// Rangs entiers supérieurs pour p50, p95, p99 et maximum.
fn quantiles(mut samples: [u128; 256]) -> [u128; 4] {
    samples.sort_unstable();
    [samples[127], samples[243], samples[253], samples[255]]
}
#[test]
fn endurance_256_et_mesures() {
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(measure)
        .unwrap()
        .join()
        .unwrap()
}
fn measure() {
    #[cfg(target_os = "macos")]
    {
        unsafe extern "C" {
            fn pthread_self() -> *mut std::ffi::c_void;
            fn pthread_get_stacksize_np(t: *mut std::ffi::c_void) -> usize;
        }
        let reserved = unsafe { pthread_get_stacksize_np(pthread_self()) };
        println!("pile_reservee={reserved}");
        assert!((128 * 1024..=192 * 1024).contains(&reserved));
    }
    let anchor = 0u8;
    ANCHOR.with(|v| v.set(black_box(&anchor) as *const u8 as usize));
    let (p, _, ph) = sample(lab::policy);
    let (c, _, ch) = sample(|| catalog(&p, 64));
    let (ix, objects) = c.memory_bytes();
    assert_eq!(ch as usize, ix + objects);
    let (mut m, _, mh) = sample(|| sim(&c, 128));
    let mut elapsed = [0; 256];
    for (i, sample) in elapsed.iter_mut().enumerate() {
        let start = Instant::now();
        m.refresh(i as u64 + 2, true, (0..128).map(node)).unwrap();
        *sample = start.elapsed().as_nanos();
    }
    let d = m.discover(sl(0)).unwrap();
    let (_, refresh_peak, refresh_delta) =
        sample(|| m.refresh(258, true, (0..128).map(node)).unwrap());
    println!(
        "pic_remplacement_inventaire={refresh_peak} delta_remplacement={refresh_delta} inventaire_tete={} groupe_unique={}",
        size_of::<Inventory>(),
        m.registry_bytes()
    );
    for i in 1..32 {
        m.discover(sl(i)).unwrap();
    }
    let mut session = Session::open(&mut m, d);
    let (v, select_peak, select_live) = sample(|| checked(&mut m, d).unwrap());
    let plan_table = m.plan_slots_bytes();
    let second = m.group(sl(1)).unwrap().stamp();
    let (second, second_peak, second_live) = sample(|| checked(&mut m, second).unwrap());
    println!(
        "deux_plans table={} premier_tas={select_live} supplement={second_live} second_pic={second_peak}",
        m.plan_slots_bytes()
    );
    m.step(second, Event::Invalidate).unwrap();
    let (r, _, _) = sample(|| m.reserve(v, 16, 1, 1).unwrap());
    let grants = m.grant_bytes();
    let rec = m.step(r, Event::Uncertain).unwrap();
    let q = m.step(rec, Event::Quarantine).unwrap();
    let retained = m.group(sl(0)).unwrap().held();
    assert_eq!(retained.dma, 22);
    let l = m.group(sl(0)).unwrap().lease().unwrap();
    m.evidence(proof(l, 1, PAGES)).unwrap();
    let rec = m.step(q, Event::Reconcile).unwrap();
    let end = m.step(rec, Event::Return).unwrap();
    let readmit = m.step(end, Event::Readmit).unwrap();
    let d = m.step(readmit, Event::Rediscover).unwrap();
    let (mut s, _) = session.cycle(&mut m, d, 2);
    let base = m.usage().unwrap();
    let mut cycles = [0; 256];
    let mut selects = [0; 256];
    DEPTH.with(|d| d.set(0));
    let fixed = LIVE.with(Cell::get);
    let (_, peak, delta) = sample(|| {
        for i in 0..256 {
            let start = Instant::now();
            let result = session.cycle(&mut m, s, i as u64 + 3);
            cycles[i] = start.elapsed().as_nanos();
            selects[i] = result.1;
            s = result.0;
            assert_eq!(m.group(sl(0)).unwrap().state(), State::Discovered);
            assert!(m.group(sl(0)).unwrap().lease().is_none());
            assert!(m.plan(sl(0)).is_none());
            assert_eq!(m.usage().unwrap(), base);
            assert_eq!(LIVE.with(Cell::get), fixed);
        }
    });
    let depth = DEPTH.with(Cell::get);
    ANCHOR.with(|v| v.set(0));
    assert_eq!(delta, 0);
    println!(
        "catalogue_index={ix} objets={objects} politique_tas={ph} politique_tete={} catalogue_tete={}",
        size_of::<Policy>(),
        size_of::<Catalog>()
    );
    println!(
        "gestionnaire_tete={} gestionnaire_tas_initial={mh} inventaire={} noeud={} groupe={} registre={} liens_recus={:?} plans_table={plan_table} selection_pic={select_peak} selection_retenue={select_live} roles={grants}",
        size_of::<Manager>(),
        m.inventory().bytes(),
        size_of::<Node>(),
        size_of::<Group>(),
        m.registry_bytes(),
        m.ipc_bytes()
    );
    println!("quarantaine={retained:?} repos={base:?}");
    println!(
        "endurance=256 delta={delta} pic_temporaire={peak} profondeur_echantillonnee={depth} inventaire_ns={:?} selection_ipc_ns={:?} cycle_ns={:?}",
        quantiles(elapsed),
        quantiles(selects),
        quantiles(cycles)
    );
}

#[test]
fn selection_un_et_soixante_quatre_paquets() {
    let policy = lab::policy();
    for packets in [1, 64] {
        let catalog = catalog(&policy, packets);
        for groups in [1, 32] {
            let mut manager = sim(&catalog, groups);
            for i in 0..groups {
                manager.discover(sl(i)).unwrap();
            }
            let mut times = [0; 256];
            // Même cible, horloge autour de la sélection seule, hors invalidation.
            let mut stamp = manager.group(sl(groups - 1)).unwrap().stamp();
            for time in &mut times {
                let start = Instant::now();
                stamp = checked(&mut manager, stamp).unwrap();
                *time = start.elapsed().as_nanos();
                stamp = manager.step(stamp, Event::Invalidate).unwrap();
            }
            println!(
                "recherche paquets={packets} groupes={groups} ns={:?}",
                quantiles(times)
            );
        }
    }
}

// Coût CPU du thread appelant. Le chargement du catalogue et la sélection
// restent en dehors des intervalles chronométrés de réserve et de réconciliation.
#[cfg(target_os = "macos")]
fn cpu_ns() -> u128 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // Sûreté : l'horloge Darwin copie un timespec dans l'objet local valide.
    assert_eq!(
        unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) },
        0
    );
    t.tv_sec as u128 * 1_000_000_000 + t.tv_nsec as u128
}
#[cfg(target_os = "macos")]
#[test]
fn cout_cpu_registre_borne() {
    use novaos_gestionnaire_sim::dma::PageSet;
    for n in [22, 128, 129, 258, 259, 260] {
        let p = lab::policy();
        let mut c = Catalog::new(&p);
        let raw = lab::RAW.replace("dma_pages=22", &format!("dma_pages={n}"));
        c.admit(
            raw.as_bytes(),
            &lab::sign(raw.as_bytes(), 1),
            &lab::objects(),
        )
        .unwrap();
        let mut m = sim(&c, 1);
        let mut d = m.discover(sl(0)).unwrap();
        let mut target = lab::target(if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else {
            "x86_64"
        });
        target.attachment = "dev-000";
        target.revision = m.inventory().revision();
        target.limits.dma_pages = n;
        let mut reserve = [0u128; 256];
        let mut evidence = [0u128; 256];
        let mut clock = [0u128; 256];
        for i in 0..256 {
            let s = m.select(d, &target, None, 1).unwrap();
            let start = cpu_ns();
            clock[i] = cpu_ns() - start;
            let start = cpu_ns();
            let result = m.reserve(s, 16, i as u64 + 1, 17);
            reserve[i] = cpu_ns() - start;
            if n == 260 {
                assert_eq!(result, Err(Error::Full));
                assert!(m.group(sl(0)).unwrap().lease().is_none());
                assert_eq!(m.dma_registry_heap(), 0);
                break;
            }
            let r = result.unwrap();
            assert_eq!(m.dma_registry_heap(), 3 * n as usize);
            let l = m.group(sl(0)).unwrap().lease().unwrap();
            let rec = m.step(r, Event::Uncertain).unwrap();
            let all = PageSet::range(0, n as usize).unwrap();
            let receipt = Observation {
                lease: l,
                serial: 1,
                source_stopped: true,
                stopped_pages: PageSet::default(),
                translations_removed: all,
                ownership_returned: all,
            };
            let start = cpu_ns();
            m.evidence(receipt).unwrap();
            evidence[i] = cpu_ns() - start;
            let returned = m.step(rec, Event::Return).unwrap();
            let readmit = m.step(returned, Event::Readmit).unwrap();
            d = m.step(readmit, Event::Rediscover).unwrap();
            assert_eq!(m.dma_registry_heap(), 0);
        }
        if n <= 259 {
            println!(
                "dma_frames={n} heap={} group={} receipt_bytes={} samples=256 cpu_ns_reserve={:?} cpu_ns_evidence={:?} clock_ns={:?}",
                3 * n,
                size_of::<Group>(),
                size_of::<Observation>(),
                quantiles(reserve),
                quantiles(evidence),
                quantiles(clock)
            );
        }
    }
}
