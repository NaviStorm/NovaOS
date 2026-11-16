extern crate alloc;
#[allow(dead_code)]
#[path = "../../libraries/documentation.rs"]
mod documentation;
mod engine;
use engine::{Action, Mode, Streams};
use std::{
    ffi::{CStr, CString, c_char},
    io::{self, Write},
};

unsafe extern "C" {
    fn tee_utf8() -> bool;
    fn tee_signals(interrupt: bool, pipe_errors: bool);
    fn tee_open(name: *const c_char, append: bool) -> i32;
    fn tee_read(data: *mut u8, len: usize) -> i32;
    fn tee_write(fd: i32, data: *const u8, len: usize) -> i32;
    fn tee_close(fd: i32) -> i32;
    fn tee_watch(fd: i32) -> i32;
    fn tee_broken(code: i32) -> bool;
    fn strerror(code: i32) -> *const c_char;
}
fn value(n: i32) -> Result<i32, i32> {
    if n < 0 { Err(-n) } else { Ok(n) }
}
struct Darwin;
impl Streams for Darwin {
    type Sink = i32;
    type Error = i32;
    fn stdout(&mut self) -> i32 {
        1
    }
    fn open(&mut self, _: usize, name: &[u8], append: bool) -> Result<i32, i32> {
        let name = CString::new(name).unwrap();
        value(unsafe { tee_open(name.as_ptr(), append) })
    }
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, i32> {
        value(unsafe { tee_read(buf.as_mut_ptr(), buf.len()) }).map(|n| n as usize)
    }
    fn write(&mut self, fd: &mut i32, data: &[u8]) -> Result<(), i32> {
        value(unsafe { tee_write(*fd, data.as_ptr(), data.len()) }).map(|_| ())
    }
    fn close(&mut self, fd: i32) -> Result<(), i32> {
        value(unsafe { tee_close(fd) }).map(|_| ())
    }
    fn broken(&self, _: &i32, e: i32) -> bool {
        unsafe { tee_broken(e) }
    }
    fn reader_gone(&mut self, fd: &i32) -> Result<bool, i32> {
        value(unsafe { tee_watch(*fd) }).map(|v| v != 0)
    }
    fn report(&mut self, name: &[u8], e: i32) {
        let mut err = io::stderr().lock();
        let _ = err.write_all(b"tee: ");
        let raw = matches!(name, b"read error" | b"iopoll error" | b"standard input");
        if raw
            || (!name.is_empty()
                && name
                    .iter()
                    .all(|b| b.is_ascii_alphanumeric() || b"_./-".contains(b)))
        {
            let _ = err.write_all(name);
        } else {
            let mut start = 0;
            while start < name.len() {
                let printable = (32..127).contains(&name[start]);
                let size = name[start..]
                    .iter()
                    .take_while(|&&b| (32..127).contains(&b) == printable)
                    .count();
                let part = &name[start..start + size];
                if printable {
                    let _ = write!(
                        err,
                        "'{}'",
                        String::from_utf8_lossy(part).replace('\'', "'\"'\"'")
                    );
                } else {
                    let _ = err.write_all(b"$'");
                    for b in part {
                        let _ = write!(err, "\\{b:03o}");
                    }
                    let _ = err.write_all(b"'");
                }
                start += size;
            }
            if name.is_empty() {
                let _ = err.write_all(b"''");
            }
        }
        let _ = writeln!(
            err,
            ": {}",
            unsafe { CStr::from_ptr(strerror(e)) }.to_string_lossy()
        );
    }
}
// Le lanceur C garantit argc pointeurs vers des chaînes terminées par NUL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn tee_main(argc: i32, argv: *const *const c_char) -> i32 {
    let args: Vec<_> = unsafe { std::slice::from_raw_parts(argv, argc as usize) }
        .iter()
        .skip(1)
        .map(|p| unsafe { CStr::from_ptr(*p) }.to_bytes())
        .collect();
    match engine::parse(&args, std::env::var_os("POSIXLY_CORRECT").is_some()) {
        Err(msg) => {
            if let Some((kind, quoted)) = msg.split_once(" argument '")
                && let Some(value) = quoted.strip_suffix("' for '--output-error'")
            {
                let (left, right) = if unsafe { tee_utf8() } {
                    ("‘", "’")
                } else {
                    ("'", "'")
                };
                let _ = writeln!(
                    io::stderr(),
                    "tee: {kind} argument {left}{value}{right} for {left}--output-error{right}\nValid arguments are:"
                );
                for name in ["warn", "warn-nopipe", "exit", "exit-nopipe"] {
                    let _ = writeln!(io::stderr(), "  - {left}{name}{right}");
                }
            } else {
                let _ = writeln!(io::stderr(), "tee: {msg}");
            }
            let _ = writeln!(io::stderr(), "Try 'tee --help' for more information.");
            1
        }
        Ok(action @ (Action::Help | Action::Version)) => {
            let tag = ["LC_ALL", "LC_MESSAGES", "LANG"]
                .iter()
                .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()));
            let lang = documentation::Language::from_tag(tag.as_deref());
            let page = documentation::lookup("tee").unwrap();
            let text = if matches!(action, Action::Help) {
                page.render(&[], false, lang).unwrap()
            } else {
                page.version(lang)
            };
            i32::from(Darwin.write(&mut 1, text.as_bytes()).is_err())
        }
        Ok(Action::Copy(opts)) => {
            unsafe { tee_signals(opts.ignore_interrupts, opts.mode != Mode::Default) };
            let mut result = engine::copy(&opts, &mut Darwin, &mut [0u8; 1024]);
            if let Err(e) = value(unsafe { tee_close(0) }) {
                Darwin.report(b"standard input", e);
                result = 1;
            }
            result
        }
    }
}
