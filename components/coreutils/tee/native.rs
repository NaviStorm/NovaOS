#![no_std]
#![no_main]
extern crate alloc;
mod native_io;
use native_io::Native;
use nova_cli::{
    console, documentation,
    fileapi::Client,
    process, runtime as rt,
    tee::{self, Action},
    tty,
};

fn execute() -> rt::Result<u64> {
    let args = process::arguments()?;
    let words: alloc::vec::Vec<_> = args.iter().map(|a| a.as_bytes()).collect();
    match tee::parse(&words, false) {
        Err(msg) => {
            console::printable(tty::ERROR, &alloc::format!("tee: {msg}\n"))?;
            Ok(1)
        }
        Ok(action @ (Action::Help | Action::Version)) => {
            console::document(
                documentation::lookup("tee").unwrap(),
                &args,
                matches!(action, Action::Help),
            )?;
            Ok(0)
        }
        Ok(Action::Copy(opts)) => {
            let input = rt::config(2);
            if input == 0 {
                return Err(rt::DENIED);
            }
            let files = if opts.files.is_empty() {
                Err(rt::DENIED)
            } else {
                Client::scoped()
            };
            let mut io = Native {
                input,
                files,
                output: rt::config(1),
            };
            let result = tee::copy(&opts, &mut io, &mut [0; process::PIPE_FRAME]);
            let closed = rt::value(rt::call(rt::IPC_CLOSE, input, 0, 0));
            if result == 0 {
                closed?;
            }
            Ok(result as u64)
        }
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let code = execute().unwrap_or_else(|e| {
        let _ = console::write(tty::ERROR, &alloc::format!("tee: {}\n", console::reason(e)));
        e
    });
    rt::exit(code)
}
