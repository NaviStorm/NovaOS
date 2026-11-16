#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
extern crate alloc;
#[cfg(not(target_os = "none"))]
mod darwin;
mod engine;
#[cfg(any(not(target_os = "none"), feature = "temoin"))]
#[path = "../../libraries/documentation_locale.rs"]
mod locale;
#[cfg(all(target_os = "none", not(feature = "temoin")))]
mod native;
mod options;
#[cfg(all(target_os = "none", feature = "temoin"))]
mod temoin;
fn help(fr: bool) -> &'static str {
    if fr {
        include_str!("../cmp.fr")
    } else {
        include_str!("../cmp.en")
    }
}
const VERSION: &str = concat!("cmp (NovaOS) ", env!("CARGO_PKG_VERSION"), "\n");

use alloc::{format, string::String};
use engine::{Input, Output};
use options::{Action, parse};
trait Platform: Output {
    type Source: Input;
    fn open(&mut self, name: &[u8], slot: usize) -> Result<Self::Source, String>;
    fn block(&self) -> usize;
    fn null_output(&self) -> bool {
        false
    }
}
fn execute(
    io: &mut impl Platform,
    args: &[&[u8]],
    invocation: &[u8],
    fr: bool,
    posix: bool,
) -> Result<u8, String> {
    let action = parse(args, invocation, posix)
        .map_err(|reason| format!("{reason}\ncmp: Try 'cmp --help' for more information."))?;
    let o = match action {
        Action::Help => {
            io.emit(false, help(fr).as_bytes())?;
            return Ok(0);
        }
        Action::Version => {
            io.emit(false, VERSION.as_bytes())?;
            return Ok(0);
        }
        Action::Compare(options) => options,
    };
    let mut open = |i| match io.open(o.names[i], i) {
        Ok(source) => Ok(Some(source)),
        Err(_) if o.quiet => Ok(None),
        Err(e) => Err(e),
    };
    let Some(a) = open(0)? else { return Ok(2) };
    if o.names[0] == o.names[1]
        && (o.skip[0] == o.skip[1] || o.names[0] == b"-" && a.remaining().is_none())
    {
        return Ok(0);
    }
    let Some(b) = open(1)? else { return Ok(2) };
    let block = io.block();
    let null = io.null_output();
    engine::compare(&o, [a, b], io, block, "char", null)
}
#[cfg(not(target_os = "none"))]
fn main() -> std::process::ExitCode {
    darwin::run().into()
}
