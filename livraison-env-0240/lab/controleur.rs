struct Cas {
    argv: &'static [&'static [u8]],
    port: u8,
    statut: u64,
}
const CAS: &[Cas] = include!("cas_env.rs");

pub(crate) fn run_example() {
    use crate::terminal::{ARG_MAX, HEADER};
    let slots = with(|s| s.audit());
    hal::log(format_args!("ENV HZ {}", hal::timer::counter_hz()));
    for (id, case) in CAS.iter().enumerate() {
        let mut data = [0u8; HEADER + ARG_MAX];
        let mut end = HEADER + 1;
        data[HEADER] = case.argv.len() as u8;
        for arg in case.argv {
            data[end..end+2].copy_from_slice(&(arg.len() as u16).to_le_bytes());
            end += 2;
            data[end..end+arg.len()].copy_from_slice(arg);
            end += arg.len();
        }
        data[32..40].copy_from_slice(&((end-HEADER) as u64).to_le_bytes());
        hal::log(format_args!("ENV BEGIN {id}"));
        let start = hal::timer::counter();
        let (child, prep, (data_pages, stack_pages)) = {
            let _gate = Guard::new();
            let child = spawn::launch_thread_witness(&data[..end]).expect("ELF env");
            let pages = with(|s| {
                assert!(!s.ipc.locals.has(child.0.slot) || s.ipc.locals.get(child.0.slot).entries.iter().all(|e| e.key == 0));
                let proc = s.processes.get_mut(child.0);
                assert_eq!(proc.terminal,0);
                proc.terminal = case.port;
                proc.space.as_ref().unwrap().page_counts()
            });
            (child,hal::timer::counter().wrapping_sub(start),pages)
        };
        let outcome = child.join();
        let elapsed = hal::timer::counter().wrapping_sub(start);
        assert_eq!(outcome,Outcome::Exit(case.statut), "cas {id}");
        hal::log(format_args!("ENV END {id} rc={} cycles={elapsed} prep={prep} data={data_pages} stack={stack_pages}", case.statut));
    }
    assert!(with(|s| s.processes.slots.iter().all(Option::is_none) && s.audit() == slots));
    hal::log(format_args!("ENV MATRICE {} OK",CAS.len()));
}
