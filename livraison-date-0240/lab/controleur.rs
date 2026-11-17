// Corps du scénario SDK dans la copie de qualification. Le lanceur SDK
// vérifie l'identité de l'image comme dans les essais du dépôt.
pub(crate) static TRACER: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(true);

pub(crate) fn run_example() {
    let mut input = &include_bytes!("date-argv.bin")[..];
    let mut cases = 0;
    while !input.is_empty() {
        let size = u16::from_le_bytes(input[..2].try_into().unwrap()) as usize;
        let ports = input[2];
        let data = &input[3..3 + size];
        input = &input[3 + size..];
        let child = {
            let _guard = Guard::new();
            let child = spawn::launch_thread_witness(data).expect("ELF date du catalogue de test");
            with(|s| s.processes.get_mut(child.0).terminal = ports);
            hal::log(format_args!(
                "DATEBEGIN {cases} pid={} ports={ports}",
                child.0.number
            ));
            child
        };
        let result = child.join();
        hal::log(format_args!("DATEEND {cases} {result:?}"));
        cases += 1;
    }
    hal::log(format_args!("DATETERM {cases}"));
}
