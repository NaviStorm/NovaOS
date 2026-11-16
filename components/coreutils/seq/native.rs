#![no_std]
#![no_main]
extern crate alloc;
mod engine;
mod format;
mod number;
use engine::{Action, Failure};
use nova_cli::{console, documentation, process, runtime as rt, seq_options as options, tty};

fn execute() -> rt::Result<()> {
    let args = process::arguments()?;
    let words: alloc::vec::Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
    let pipe = rt::config(1);
    let result = engine::execute(&words, &number::C, |bytes| {
        if pipe != 0 {
            process::pipe_write(pipe, bytes)
                .map(|_| ())
                .map_err(|(_, code)| code)
        } else {
            console::write(
                tty::OUTPUT,
                core::str::from_utf8(bytes).map_err(|_| rt::BAD_ARGUMENT)?,
            )
        }
    });
    let status = match result {
        Ok(Action::Done) => Ok(()),
        Ok(action) => console::document(
            documentation::lookup("seq").unwrap(),
            &args,
            matches!(action, Action::Help),
        ),
        Err(Failure::Arguments(s)) => {
            console::printable(
                tty::ERROR,
                &alloc::format!("seq: {}\n", alloc::string::String::from_utf8_lossy(&s.0)),
            )?;
            Err(1)
        }
        Err(Failure::Write(code)) => {
            let _ = console::write(
                tty::ERROR,
                &alloc::format!("seq: {}\n", console::reason(code)),
            );
            Err(1)
        }
    };
    if pipe != 0 {
        let closed = rt::value(rt::call(rt::IPC_CLOSE, pipe, 0, 0)).map(|_| ());
        status.and(closed)
    } else {
        status
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(execute().err().unwrap_or(0))
}
