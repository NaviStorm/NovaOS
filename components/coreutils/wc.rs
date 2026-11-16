#![no_std]
#![no_main]
extern crate alloc;
mod common;
mod count;
mod text;
// Sûreté : point d’entrée unique de cet ELF, appelé avec sa pile utilisateur.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    use nova_cli::{console, process, runtime as rt, tty};
    let pipe = rt::config(2);
    if pipe != 0 {
        let result = (|| {
            if process::arguments()?.as_slice() != ["-l"] {
                return Err(rt::BAD_ARGUMENT);
            }
            let mut count = 0u64;
            let mut buf = [0; process::PIPE_FRAME];
            loop {
                let len = process::pipe_read(pipe, &mut buf)?;
                if len == 0 { break; }
                let lines = buf[..len].iter().filter(|&&b| b == b'\n').count();
                count = count.checked_add(lines as u64).ok_or(rt::LIMIT)?;
            }
            console::write(tty::OUTPUT, &alloc::format!("{count}\n"))
        })();
        if let Err(e) = result {
            let _ = console::write(tty::ERROR, &alloc::format!("wc : {}\n", console::reason(e)));
            rt::exit(e);
        }
        rt::exit(0);
    }
    common::run(nova_cli::command::Program::Wc, count::command)
}
