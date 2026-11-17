use crate::{
    number,
    options::{self, Action},
    quote,
};
use alloc::{format, vec::Vec};

pub enum Request {
    Help,
    Version,
    Wait(f64),
}

pub fn parse(
    args: &[&[u8]],
    point: &str,
    utf8: bool,
    posix: bool,
    invocation: &[u8],
) -> Result<Request, Vec<u8>> {
    let mut messages = Vec::new();
    match options::options(args.iter().copied(), posix) {
        Ok(Action::Help) => return Ok(Request::Help),
        Ok(Action::Version) => return Ok(Request::Version),
        Err(error) => messages.extend([invocation, b": ", &error, b"\n"].concat()),
        Ok(Action::Wait(separator)) => {
            if args.is_empty() {
                messages.extend_from_slice(b"sleep: missing operand\n");
            }
            let mut total = 0.0;
            for (i, arg) in args.iter().enumerate() {
                if Some(i) == separator {
                    continue;
                }
                if let Some(seconds) = number::seconds(arg, point) {
                    total = number::upper(total + seconds);
                } else {
                    messages.extend(
                        format!("sleep: invalid time interval {}\n", quote::quote(arg, utf8))
                            .bytes(),
                    );
                }
            }
            if messages.is_empty() {
                return Ok(Request::Wait(total));
            }
        }
    }
    messages.extend(
        [
            &b"Try '"[..],
            invocation,
            b" --help' for more information.\n",
        ]
        .concat(),
    );
    Err(messages)
}

// Au-delà du domaine de time_t signé, GNU attend un signal.
pub fn nanos(seconds: f64) -> Option<u128> {
    if seconds >= (1u64 << 63) as f64 {
        return None;
    }
    let whole = seconds as u64;
    let fraction = (seconds - whole as f64) * 1e9;
    let ns = fraction as u64;
    Some(u128::from(whole) * 1_000_000_000 + u128::from(ns + u64::from((ns as f64) < fraction)))
}
