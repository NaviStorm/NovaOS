// Compilé uniquement dans la copie du noyau de recette, une seule vCPU.
pub(crate) static TICKS_ENFANT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

pub(crate) fn run_example() {
    use core::sync::atomic::Ordering::Relaxed;
    let mut input = &include_bytes!("sleep-argv.bin")[..];
    let hz = hal::timer::counter_hz();
    let mut cases = 0;
    let initial = heap::audit(true);
    let slots = with(|s| s.audit());
    while !input.is_empty() {
        let size = u16::from_le_bytes(input[..2].try_into().unwrap()) as usize;
        let (ports, cancel) = (input[2], input[3] == 1);
        let data = &input[4..4 + size];
        input = &input[4 + size..];
        hal::log(format_args!("SLEEPBEGIN {cases}"));
        TICKS_ENFANT.store(0, Relaxed);
        let start = hal::timer::counter();
        let child = {
            let _guard = Guard::new();
            let child = spawn::launch_thread_witness(data).expect("ELF sleep");
            with(|s| s.processes.get_mut(child.0).terminal = ports);
            child
        };
        let pid = child.0;
        if cancel {
            until(|| {
                with(|s| {
                    s.slots.iter().flatten().any(|t| {
                        t.owner == Some(pid) && matches!(t.state, State::Blocked(Wait::Ipc(_)))
                    })
                })
            });
            sleep(5);
            with(|s| stop_process(s, pid, Outcome::Exit(125), hal::timer::counter()));
        }
        let result = child.join();
        let elapsed = hal::timer::counter() - start;
        let cpu = TICKS_ENFANT.load(Relaxed);
        assert!(with(|s| s.processes.slots.iter().all(Option::is_none)
            && s.ipc.pages() == 0
            && s.audit() == slots));
        assert_eq!(heap::audit(true), initial);
        hal::log(format_args!(
            "SLEEPEND {cases} pid={} {result:?} {elapsed} {hz} {cpu}",
            pid.number
        ));
        cases += 1;
    }
    hal::log(format_args!("SLEEPTERM {cases}"));
}
