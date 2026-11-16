#![no_std]
#![no_main]
extern crate alloc;
mod common;
mod text;
// Sûreté : point d’entrée unique de cet ELF, appelé avec sa pile utilisateur.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    use nova_cli::{console, process, runtime as rt, tty};
    let pipe = rt::config(2);
    if pipe != 0 {
        let result = (|| {
            if process::arguments()?.as_slice() != ["-n", "1"] {
                return Err(rt::BAD_ARGUMENT);
            }
            text::scan_chunks::<{ process::PIPE_FRAME + 3 }>(|buf| {
                let len = process::pipe_read(pipe, buf)?;
                if let Some(lf) = buf[..len].iter().position(|&b| b == b'\n') {
                    rt::value(rt::call(rt::IPC_CLOSE, pipe, 0, 0))?;
                    return Ok((lf + 1, true));
                }
                Ok((len, len == 0))
            }, |part| {
                match part {
                    Ok(s) => console::printable(tty::OUTPUT, s)?,
                    Err(b) => console::write(tty::OUTPUT, &alloc::format!("\\x{b:02x}"))?,
                }
                Ok(true)
            })
        })();
        if let Err(code) = result {
            let _ = console::write(tty::ERROR, &alloc::format!("head : {}\n", console::reason(code)));
            rt::exit(code);
        }
        rt::exit(0);
    }
    common::run(nova_cli::command::Program::Head, text::command)
}
