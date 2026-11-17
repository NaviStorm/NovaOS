#![no_main]
extern crate alloc;
#[allow(dead_code)]
#[path = "../../libraries/documentation.rs"]
mod documentation;
mod engine;
mod number;
mod options;
mod quote;
use std::{
    ffi::{CStr, c_char},
    io::{self, Write},
    time::Duration,
};
unsafe extern "C" {
    fn setlocale(category: i32, name: *const c_char) -> *const c_char;
    fn localeconv() -> *const *const c_char;
    fn write(fd: i32, data: *const std::ffi::c_void, len: usize) -> isize;
    fn pause() -> i32;
    fn strerror(code: i32) -> *const c_char;
}
fn output(mut data: &[u8]) -> io::Result<()> {
    while !data.is_empty() {
        let n = unsafe { write(1, data.as_ptr().cast(), data.len()) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        data = &data[n as usize..];
    }
    Ok(())
}
// Entrée libc pour conserver SIGPIPE et les descripteurs hérités de l’appelant.
#[unsafe(no_mangle)]
unsafe extern "C" fn main(argc: i32, argv: *const *const c_char) -> i32 {
    let args: Vec<_> = unsafe { std::slice::from_raw_parts(argv, argc as usize) }
        .iter()
        .map(|&p| unsafe { CStr::from_ptr(p) }.to_bytes())
        .collect();
    let (point, utf8) = unsafe {
        setlocale(0, c"".as_ptr());
        let point = CStr::from_ptr(*localeconv()).to_str().unwrap_or(".");
        let ctype = CStr::from_ptr(setlocale(2, core::ptr::null())).to_string_lossy();
        (point, ctype.to_ascii_lowercase().contains("utf"))
    };
    match engine::parse(
        &args[1..],
        point,
        utf8,
        std::env::var_os("POSIXLY_CORRECT").is_some(),
        args[0],
    ) {
        Ok(engine::Request::Wait(seconds)) => match engine::nanos(seconds) {
            Some(ns) => std::thread::sleep(Duration::new(
                (ns / 1_000_000_000) as u64,
                (ns % 1_000_000_000) as u32,
            )),
            None => loop {
                unsafe { pause() };
            },
        },
        Ok(doc) => {
            let tag = ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|k| std::env::var(k).ok().filter(|v| !v.is_empty()));
            let language = documentation::Language::from_tag(tag.as_deref());
            let page = documentation::lookup("sleep").unwrap();
            let bytes = if matches!(doc, engine::Request::Help) {
                page.render(&[], false, language).unwrap()
            } else {
                page.version(language)
            };
            if let Err(error) = output(bytes.as_bytes()) {
                let reason = unsafe { CStr::from_ptr(strerror(error.raw_os_error().unwrap_or(5))) };
                let _ = io::stderr()
                    .write_all(&[&b"sleep: write error: "[..], reason.to_bytes(), b"\n"].concat());
                return 1;
            }
        }
        Err(message) => {
            let _ = io::stderr().write_all(&message);
            return 1;
        }
    }
    0
}
