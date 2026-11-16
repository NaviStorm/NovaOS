#![no_std]
#![no_main]
extern crate alloc;
mod native_stream;
use nova_cli::tr as engine;
#[allow(dead_code)]
#[path = "../text.rs"]
mod text;
use nova_cli::{console, documentation, process, runtime as rt, tty};

fn execute() -> rt::Result<()> {
    let args = process::arguments()?;
    let words: alloc::vec::Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
    let mut warnings = alloc::vec::Vec::new();
    let action = engine::prepare("tr", &words, false, |msg| warnings.push(msg));
    for msg in warnings {
        console::printable(tty::ERROR, &alloc::format!("tr: {msg}\n"))?;
    }
    match action {
        Err(msg) => {
            let msg = alloc::string::String::from_utf8_lossy(&msg);
            console::printable(tty::ERROR, &alloc::format!("tr: {msg}\n"))?;
            Err(1)
        }
        Ok(engine::Action::Help | engine::Action::Version) => console::document(
            documentation::lookup("tr").unwrap(),
            &args,
            matches!(action, Ok(engine::Action::Help)),
        ),
        Ok(engine::Action::Filter(mut filter)) => {
            let pipe = rt::config(2);
            if pipe == 0 {
                return Err(rt::DENIED);
            }
            let result = text::scan_chunks::<{ process::PIPE_FRAME + 3 }>(
                |buf| {
                    native_stream::read(&mut filter, buf, |frame| process::pipe_read(pipe, frame))
                },
                |part| {
                    match part {
                        Ok(s) => console::printable(tty::OUTPUT, s)?,
                        Err(b) => console::write(tty::OUTPUT, &alloc::format!("\\x{b:02x}"))?,
                    }
                    Ok(true)
                },
            );
            native_stream::finish(result, || rt::value(rt::call(rt::IPC_CLOSE, pipe, 0, 0)))
        }
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let status = execute().err().unwrap_or(0);
    if status > 1 {
        let _ = console::write(
            tty::ERROR,
            &alloc::format!("tr : {}\n", console::reason(status)),
        );
    }
    rt::exit(status)
}
