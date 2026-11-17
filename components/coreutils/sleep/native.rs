#![no_std]
#![no_main]
extern crate alloc;
mod engine;
mod number;
mod options;
mod quote;
use nova_cli::{console, documentation, process, runtime as rt, tty};

fn wait(seconds: f64) -> rt::Result<()> {
    let mut remaining = engine::nanos(seconds).map(|ns| ns.div_ceil(4_000_000));
    if remaining == Some(0) {
        return Ok(());
    }
    let channel = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
    let result = loop {
        // Un tick supplémentaire couvre la phase inconnue du premier tick.
        let count = remaining.unwrap_or(14999).min(14999);
        match rt::receive_for(channel, count as u64 + 1) {
            Err(rt::TIMED_OUT) => {
                if let Some(left) = &mut remaining {
                    *left -= count;
                    if *left == 0 {
                        break Ok(());
                    }
                }
            }
            Err(code) => break Err(code),
            Ok(_) => break Err(rt::BAD_ARGUMENT),
        }
    };
    let close = rt::value(rt::call(rt::IPC_CLOSE, channel, 0, 0)).map(|_| ());
    result.and(close)
}
fn execute() -> rt::Result<()> {
    let args = process::arguments()?;
    let words: alloc::vec::Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
    let result = match engine::parse(&words, ".", false, false, b"sleep") {
        Ok(engine::Request::Wait(seconds)) => wait(seconds),
        Ok(doc) => console::document(
            documentation::lookup("sleep").unwrap(),
            &args,
            matches!(doc, engine::Request::Help),
        ),
        Err(message) => {
            console::printable(
                tty::ERROR,
                &alloc::string::String::from_utf8_lossy(&message),
            )?;
            return Err(1);
        }
    };
    result.map_err(|code| {
        let _ = console::write(
            tty::ERROR,
            &alloc::format!("sleep: {}\n", console::reason(code)),
        );
        1
    })
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(execute().err().unwrap_or(0))
}
