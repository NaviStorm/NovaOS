use super::*;
use nova_cli::{process, runtime as rt, tty};

struct Ports;
impl Output for Ports {
    fn write(&mut self, err: bool, bytes: &[u8]) -> Result<(), ()> {
        let port = if err { tty::ERROR } else { tty::OUTPUT };
        // Keep valid UTF-8 scalars together on the text console. Its own policy
        // decides which byte sequences are admissible on the assigned port.
        let mut rest = bytes;
        while !rest.is_empty() {
            let mut n = rest.len().min(rt::DEBUG_MAX);
            if n < rest.len() {
                while n > 0 && rest[n] & 0xc0 == 0x80 {
                    n -= 1
                }
                if n == 0 {
                    n = rest.len().min(rt::DEBUG_MAX)
                }
            }
            rt::value(rt::call(
                tty::CONSOLE,
                port.into(),
                rest.as_ptr() as u64,
                n as u64,
            ))
            .map_err(|_| ())?;
            rest = &rest[n..];
        }
        Ok(())
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let mut io = Ports;
    let code = match process::arguments() {
        Ok(args) => {
            let argv: Vec<_> = args.iter().map(|s| s.as_bytes()).collect();
            engine::run(
                &mut io,
                &argv,
                Locale {
                    curved_quotes: true,
                    decode: utf8,
                    encode: encode_utf8,
                    ..Locale::default()
                },
            )
        }
        Err(_) => {
            let _ = io.write(true, b"printf: invalid argument frame\n");
            1
        }
    };
    rt::exit(code.into())
}
