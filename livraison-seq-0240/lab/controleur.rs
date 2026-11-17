// Corps du scénario SDK dans la copie de qualification. Le lanceur SDK
// vérifie l'identité de l'image comme dans les essais du dépôt.
pub(crate) static TRACER: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(true);

pub(crate) fn run_example() {
    let mut input = &include_bytes!("seq-argv.bin")[..];
    let mut cases = 0;
    while !input.is_empty() {
        let size = u16::from_le_bytes(input[..2].try_into().unwrap()) as usize;
        let ports = input[2];
        let data = &input[3..3 + size];
        input = &input[3 + size..];
        let child = {
            let _guard = Guard::new();
            let child = spawn::launch_thread_witness(data).expect("ELF seq du catalogue de test");
            with(|s| s.processes.get_mut(child.0).terminal = ports);
            hal::log(format_args!(
                "SEQBEGIN {cases} pid={} ports={ports}",
                child.0.number
            ));
            child
        };
        let result = child.join();
        hal::log(format_args!("SEQEND {cases} {result:?}"));
        cases += 1;
    }
    TRACER.store(false, core::sync::atomic::Ordering::Relaxed);
    let hz = hal::timer::counter_hz();
    for (n, arg) in [(1000u32, &b"1000"[..]), (10000, &b"10000"[..])] {
        let mut data = [0u8; 48];
        data[32..40].copy_from_slice(&(3 + arg.len() as u64).to_le_bytes());
        data[40] = 1;
        data[41..43].copy_from_slice(&(arg.len() as u16).to_le_bytes());
        data[43..43 + arg.len()].copy_from_slice(arg);
        for tour in 0..4 {
            hal::log(format_args!("SEQBENCH {n} {tour}"));
            let start = hal::timer::counter();
            let child = {
                let _guard = Guard::new();
                let child = spawn::launch_thread_witness(&data[..43 + arg.len()]).unwrap();
                with(|s| s.processes.get_mut(child.0).terminal = 6);
                child
            };
            let outcome = child.join();
            let elapsed = hal::timer::counter() - start;
            assert_eq!(outcome, Outcome::Exit(0));
            hal::log(format_args!("SEQTIME {elapsed} {hz}"));
        }
    }
    hal::log(format_args!("SEQTERM {cases}"));
}
