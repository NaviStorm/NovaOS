use crate::{
    engine::{self, Output, Result},
    format::Fields,
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use jiff::{Timestamp, Zoned, tz::TimeZone};
use nova_cli::{
    console, documentation::Language, fileapi::Client, process, protocol as fs, runtime as rt, tty,
};

fn explain(code: u64) -> String {
    console::reason(code).into()
}
pub fn zone(_: bool) -> Result<(TimeZone, String)> {
    Ok((TimeZone::UTC, "UTC0".into()))
}
pub fn now() -> Result<Timestamp> {
    let sec = rt::value(rt::call(rt::CLOCK_UTC, 0, 0, 0)).map_err(explain)?;
    Timestamp::from_second(i64::try_from(sec).map_err(|_| "RTC out of range")?)
        .map_err(|e| e.to_string())
}
pub fn resolution() -> Result<Timestamp> {
    Timestamp::from_second(1).map_err(|e| e.to_string())
}
pub fn quote(s: &str) -> String {
    engine::quote_bytes(s.as_bytes())
}
pub fn fraction_digits() -> usize {
    1
}
pub fn set(_: &Fields) -> Result<()> {
    Err("Operation not permitted".into())
}
pub fn calendar_parts(_: &Zoned) -> Option<(i64, i64, i64)> {
    None
}
pub fn localized(_: char, _: &Zoned) -> Option<String> {
    None
}

// Le profil historique transmet le seul objet sélectionné dans le handle 1.
fn selected(path: &str) -> Result<Client> {
    let code = rt::config(3);
    if code != 0 {
        return Err(format!("{path}: {}", explain(code)));
    }
    Client::scoped().map_err(|code| format!("{path}: {}", explain(code)))
}
pub fn reference(path: &str) -> Result<Timestamp> {
    let mut client = selected(path)?;
    let stat = client.simple(fs::STAT, 1).map_err(explain)?;
    if fs::length(&stat) != 73 {
        return Err(format!("{path}: modification time unknown"));
    }
    let pos = fs::PAYLOAD + 37;
    let seconds = i64::from_le_bytes(stat[pos..pos + 8].try_into().unwrap());
    let nanos = u32::from_le_bytes(stat[pos + 8..pos + 12].try_into().unwrap());
    if nanos >= 1_000_000_000 {
        return Err(format!("{path}: modification time unknown"));
    }
    Timestamp::new(seconds, nanos as i32).map_err(|e| e.to_string())
}
pub fn lines(path: &str, mut visit: impl FnMut(&str) -> Result<()>) -> Result<()> {
    let pipe = rt::config(2);
    let mut file = if path == "-" {
        if pipe == 0 {
            return Err("standard input: accès refusé".into());
        }
        None
    } else {
        Some(selected(path)?)
    };
    let mut offset = 0;
    let mut buf = [0u8; 512];
    let mut line = Vec::new();
    loop {
        let n = match &mut file {
            Some(c) => c.read(1, offset, &mut buf),
            None => process::pipe_read(pipe, &mut buf),
        }
        .map_err(explain)?;
        if n == 0 {
            if !line.is_empty() {
                visit(&String::from_utf8_lossy(&line))?
            }
            break;
        }
        offset += n as u64;
        for &b in &buf[..n] {
            if b == b'\n' {
                line.push(b);
                visit(&String::from_utf8_lossy(&line))?;
                line.clear()
            } else {
                line.try_reserve(1)
                    .map_err(|_| "insufficient memory for input line")?;
                line.push(b)
            }
        }
    }
    Ok(())
}
struct Streams(u64);
impl Output for Streams {
    fn line(&mut self, s: &str) -> Result<()> {
        if self.0 != 0 {
            process::pipe_write(self.0, s.as_bytes())
                .map(|_| ())
                .map_err(|(_, code)| explain(code))
        } else {
            console::write(tty::OUTPUT, s).map_err(explain)
        }
    }
    fn error(&mut self, s: &str) -> Result<()> {
        console::printable(tty::ERROR, s).map_err(explain)
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let args = match process::arguments() {
        Ok(v) => v,
        Err(code) => rt::exit(code),
    };
    let lang: Language = nova_cli::documentation::child_language(&args);
    let mut streams = Streams(rt::config(1));
    let mut ok = match engine::execute(&args, lang, &mut streams) {
        Ok(ok) => ok,
        Err(e) => {
            let _ = streams.error(&format!("date: {e}\n"));
            false
        }
    };
    for key in [streams.0, rt::config(2)] {
        if key != 0 {
            ok &= rt::value(rt::call(rt::IPC_CLOSE, key, 0, 0)).is_ok()
        }
    }
    rt::exit(if ok { 0 } else { 1 })
}
