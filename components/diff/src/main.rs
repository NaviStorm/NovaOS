#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
extern crate alloc;
use alloc::{
    format,
    string::{String, ToString},
    vec,
    vec::Vec,
};
mod compare;
mod custom;
mod engine;
mod options;
mod render;
use options::{Options, txt};
#[cfg(not(target_os = "none"))]
mod darwin;
#[path = "../../libraries/documentation_locale.rs"]
mod locale;
#[cfg(all(target_os = "none", not(feature = "temoin")))]
mod native;
#[cfg(all(feature = "temoin", target_os = "none"))]
mod temoin;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    File,
    Dir,
    Link,
    Other,
}
#[derive(Clone)]
struct Meta {
    kind: Kind,
    id: Option<(u64, u64)>,
    stamp: Vec<u8>,
}
impl Meta {
    fn file() -> Self {
        Self {
            kind: Kind::File,
            id: None,
            stamp: vec![],
        }
    }
}
trait Platform {
    fn stdin_child(&self, _: &[u8]) -> Result<Vec<u8>, String> {
        Err("cannot compare standard input directory".into())
    }
    fn utf8(&self) -> bool {
        false
    }
    fn dumb(&self) -> bool {
        false
    }
    fn stat(&mut self, path: &[u8], o: &Options) -> Result<Option<Meta>, String>;
    fn read(&mut self, path: &[u8]) -> Result<Vec<u8>, String>;
    fn list(&mut self, path: &[u8]) -> Result<Vec<Vec<u8>>, String>;
    fn link(&mut self, _: &[u8]) -> Result<Vec<u8>, String> {
        Err("symbolic links unavailable".into())
    }
    fn emit(&mut self, err: bool, b: &[u8]) -> Result<(), String>;
    fn paginate(&mut self, _: &[u8], _: &[u8]) -> Result<Vec<u8>, String> {
        Err("pagination unavailable".into())
    }
}
fn documentation(fr: bool, version: bool) -> Vec<u8> {
    if version {
        return if fr {
            b"diff (NovaOS) 0.24.0\nVersion native de NovaOS.\n".to_vec()
        } else {
            b"diff (NovaOS) 0.24.0\nNative NovaOS version.\n".to_vec()
        };
    }
    let s = if fr {
        include_str!("../diff.fr")
    } else {
        include_str!("../diff.en")
    };
    s.as_bytes().to_vec()
}
fn execute(
    p: &mut impl Platform,
    args: &[&[u8]],
    inv: &[u8],
    fr: bool,
    posix: bool,
    tty: bool,
) -> u8 {
    let result = (|| {
        let mut o = options::parse(args, inv, posix)?;
        o.tty = (tty || o.has("-presume-output-tty")) && !p.dumb();
        o.utf8 = p.utf8();
        if let Some(d) = o.doc {
            p.emit(false, &documentation(fr, d == "version"))?;
            return Ok(0);
        }
        compare::run(p, &o)
    })();
    result.unwrap_or_else(|msg: String| {
        let _ = p.emit(true, format!("diff: {msg}\n").as_bytes());
        2
    })
}
#[cfg(not(target_os = "none"))]
fn main() -> std::process::ExitCode {
    darwin::run().into()
}
