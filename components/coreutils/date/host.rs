use crate::bytes;
use crate::{
    documentation::Language,
    engine::{self, Output, Result},
    format::Fields,
};
use jiff::{Timestamp, Zoned, tz::TimeZone};
use std::os::unix::ffi::OsStrExt;
use std::{
    env,
    ffi::CStr,
    fs,
    io::{self, BufRead, Write},
    os::unix::fs::MetadataExt,
};

fn os_error(e: io::Error) -> String {
    e.raw_os_error()
        .map(|n| {
            unsafe { CStr::from_ptr(libc::strerror(n)) }
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_else(|| e.to_string())
}
pub fn zone(utc: bool) -> Result<(TimeZone, String)> {
    let name = if utc {
        Some("UTC0".into())
    } else {
        env::var("TZ").ok()
    };
    let Some(name) = name else {
        let z = TimeZone::system();
        return Ok((z.clone(), z.iana_name().unwrap_or("local").into()));
    };
    let z = match name.as_str() {
        "" | "UTC" | "UTC0" => TimeZone::UTC,
        s => TimeZone::get(s.trim_start_matches(':'))
            .or_else(|_| TimeZone::posix(s))
            .map_err(|e| e.to_string())?,
    };
    Ok((z, name))
}
fn clock(resolution: bool) -> Result<Timestamp> {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    let rc = unsafe {
        if resolution {
            libc::clock_getres(libc::CLOCK_REALTIME, &mut t)
        } else {
            libc::clock_gettime(libc::CLOCK_REALTIME, &mut t)
        }
    };
    if rc != 0 {
        return Err(os_error(io::Error::last_os_error()));
    }
    Timestamp::new(t.tv_sec, t.tv_nsec as i32).map_err(|e| e.to_string())
}
pub fn now() -> Result<Timestamp> {
    clock(false)
}
pub fn resolution() -> Result<Timestamp> {
    clock(true)
}
pub fn quote(s: &str) -> String {
    let bytes = bytes::decode(s);
    let ascii = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|k| env::var(k).ok().filter(|v| !v.is_empty()))
        .is_none_or(|v| v == "C" || v == "POSIX");
    if ascii || str::from_utf8(&bytes).is_err() {
        engine::quote_bytes(&bytes)
    } else {
        s.replace('\\', "\\\\").replace('\'', "\\'")
    }
}
pub fn fraction_digits() -> usize {
    let n = resolution().map_or(1, |t| t.subsec_nanosecond().max(1));
    9 - n.ilog10() as usize
}
pub fn set(f: &Fields) -> Result<()> {
    let t = libc::timespec {
        tv_sec: f.seconds,
        tv_nsec: f.zoned.subsec_nanosecond().into(),
    };
    if unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &t) } == 0 {
        Ok(())
    } else {
        Err(os_error(io::Error::last_os_error()))
    }
}
pub fn reference(path: &str) -> Result<Timestamp> {
    let m = fs::metadata(std::ffi::OsStr::from_bytes(&bytes::decode(path)))
        .map_err(|e| format!("{path}: {}", os_error(e)))?;
    Timestamp::new(m.mtime(), m.mtime_nsec() as i32).map_err(|e| e.to_string())
}
pub fn lines(path: &str, mut visit: impl FnMut(&str) -> Result<()>) -> Result<()> {
    let mut reader: Box<dyn BufRead> = if path == "-" {
        Box::new(io::stdin().lock())
    } else {
        Box::new(io::BufReader::new(
            fs::File::open(std::ffi::OsStr::from_bytes(&bytes::decode(path)))
                .map_err(|e| format!("{path}: {}", os_error(e)))?,
        ))
    };
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|e| format!("{path}: read error: {}", os_error(e)))?
            == 0
        {
            break;
        }
        visit(&bytes::encode(&line))?;
    }
    Ok(())
}
pub use crate::calendar::parts as calendar_parts;
pub fn localized(code: char, z: &Zoned) -> Option<String> {
    if let Some((loc, persian)) = crate::calendar::locale() {
        let pattern = match code {
            'b' | 'h' => Some("MMM"),
            'B' => Some("MMMM"),
            'x' => Some(if persian { "yyyy/MM/dd" } else { "dd/MM/yyyy" }),
            _ => None,
        };
        if let Some(pattern) = pattern {
            return crate::calendar::field(z, loc, pattern);
        }
    }
    let mut tm: libc::tm = unsafe { core::mem::zeroed() };
    tm.tm_year = i32::from(z.year()) - 1900;
    tm.tm_mon = i32::from(z.month()) - 1;
    tm.tm_mday = z.day().into();
    tm.tm_hour = z.hour().into();
    tm.tm_min = z.minute().into();
    tm.tm_sec = z.second().into();
    tm.tm_wday = z.weekday().to_sunday_zero_offset().into();
    tm.tm_yday = i32::from(z.date().day_of_year()) - 1;
    tm.tm_gmtoff = z.offset().seconds().into();
    let info = z.time_zone().to_offset_info(z.timestamp());
    let zone_name = std::ffi::CString::new(info.abbreviation()).ok()?;
    tm.tm_zone = zone_name.as_ptr().cast_mut();
    tm.tm_isdst = i32::from(info.dst().is_dst());
    let format = [b'%', if code == 'P' { b'p' } else { code as u8 }, 0];
    let mut buf = [0u8; 1024];
    let n = unsafe {
        libc::strftime(
            buf.as_mut_ptr().cast(),
            buf.len(),
            format.as_ptr().cast(),
            &tm,
        )
    };
    let s = core::str::from_utf8(&buf[..n]).ok()?.to_owned();
    Some(if code == 'P' { s.to_lowercase() } else { s })
}
struct Streams<W: Write>(W);
impl<W: Write> Output for Streams<W> {
    fn line(&mut self, s: &str) -> Result<()> {
        self.0.write_all(&bytes::decode(s)).map_err(os_error)
    }
    fn error(&mut self, s: &str) -> Result<()> {
        io::stderr()
            .lock()
            .write_all(&bytes::decode(s))
            .map_err(os_error)
    }
}
pub fn main() {
    unsafe { libc::setlocale(libc::LC_TIME, c"".as_ptr()) };
    let args: Vec<_> = env::args_os().skip(1).collect();
    let words: Vec<_> = args.iter().map(|s| bytes::encode(s.as_bytes())).collect();
    let words: Vec<_> = words.iter().map(|s| s.as_ref()).collect();
    let tag = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|n| env::var(n).ok().filter(|v| !v.is_empty()));
    let lang = Language::from_tag(tag.as_deref());
    let mut out = Streams(io::BufWriter::new(io::stdout().lock()));
    let result = engine::execute(&words, lang, &mut out);
    let ok = match result.and_then(|ok| out.0.flush().map(|_| ok).map_err(os_error)) {
        Ok(ok) => ok,
        Err(e) => {
            let _ = out.error(&format!("date: {e}\n"));
            false
        }
    };
    std::process::exit(if ok { 0 } else { 1 });
}
