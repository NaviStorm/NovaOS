//! Calendriers de présentation de Darwin. ICU reçoit une date murale en UTC
//! pour éviter un deuxième décalage lors de la conversion de calendrier.
use core::ffi::{c_char, c_void};
use jiff::Zoned;
use std::ffi::CStr;

#[link(name = "icucore")]
unsafe extern "C" {
    fn udat_open(
        time: i32,
        date: i32,
        locale: *const c_char,
        zone: *const u16,
        zone_len: i32,
        pattern: *const u16,
        len: i32,
        status: *mut i32,
    ) -> *mut c_void;
    fn udat_format(
        fmt: *const c_void,
        instant: f64,
        result: *mut u16,
        len: i32,
        position: *mut c_void,
        status: *mut i32,
    ) -> i32;
    fn udat_close(fmt: *mut c_void);
}
pub fn locale() -> Option<(&'static CStr, bool)> {
    let p = unsafe { libc::setlocale(libc::LC_TIME, core::ptr::null()) };
    if p.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(p) }.to_bytes();
    if bytes.starts_with(b"am_ET") {
        Some((c"am_ET@calendar=ethiopic", false))
    } else if bytes.starts_with(b"fa_IR") {
        Some((c"fa_IR@calendar=persian", true))
    } else if bytes.starts_with(b"th_TH") {
        Some((c"th_TH@calendar=buddhist", false))
    } else {
        None
    }
}
pub fn field(z: &Zoned, locale: &CStr, pattern: &str) -> Option<String> {
    let zone = [85u16, 84, 67];
    let pattern: Vec<_> = pattern.encode_utf16().collect();
    let mut status = 0;
    let f = unsafe {
        udat_open(
            -2,
            -2,
            locale.as_ptr(),
            zone.as_ptr(),
            3,
            pattern.as_ptr(),
            pattern.len() as i32,
            &mut status,
        )
    };
    if f.is_null() {
        return None;
    }
    let mut buf = [0u16; 256];
    let ms = z.timestamp().as_millisecond() as f64 + f64::from(z.offset().seconds()) * 1000.;
    let count = unsafe {
        udat_format(
            f,
            ms,
            buf.as_mut_ptr(),
            buf.len() as i32,
            core::ptr::null_mut(),
            &mut status,
        )
    };
    unsafe { udat_close(f) };
    if status > 0 {
        return None;
    }
    String::from_utf16(buf.get(..usize::try_from(count).ok()?)?).ok()
}
pub fn parts(z: &Zoned) -> Option<(i64, i64, i64)> {
    let (loc, _) = locale()?;
    let value = field(z, loc, "yyyy-MM-dd")?;
    let ascii: String = value
        .chars()
        .map(|c| match c {
            '۰'..='۹' => char::from(b'0' + (c as u32 - '۰' as u32) as u8),
            other => other,
        })
        .collect();
    let mut parts = ascii.split('-');
    Some((
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    ))
}
