use crate::options::{LIMIT, Options, quote};
use alloc::{format, string::String, vec, vec::Vec};

pub trait Input {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String>;
    fn skip(&mut self, count: u64) -> Result<(), String>;
    fn remaining(&self) -> Option<u64>;
    fn identity(&self) -> Option<(u64, u64, u64)>;
}
pub trait Output {
    fn emit(&mut self, err: bool, bytes: &[u8]) -> Result<(), String>;
}
fn fill(input: &mut impl Input, mut buf: &mut [u8]) -> Result<usize, String> {
    let mut count = 0;
    while !buf.is_empty() {
        let n = input.read(buf)?;
        if n == 0 {
            break;
        }
        count += n;
        buf = &mut buf[n..];
    }
    Ok(count)
}
fn shown(byte: u8) -> String {
    let mut s = String::new();
    if byte >= 128 {
        s.push_str("M-")
    }
    match byte & 127 {
        c @ 0..32 => {
            s.push('^');
            s.push((c + 64) as char)
        }
        127 => s.push_str("^?"),
        c => s.push(c as char),
    }
    s
}
pub fn compare<I: Input>(
    o: &Options,
    mut inputs: [I; 2],
    out: &mut impl Output,
    block: usize,
    byte_word: &str,
    null: bool,
) -> Result<u8, String> {
    let mut sizes = [None; 2];
    for (i, input) in inputs.iter_mut().enumerate() {
        input.skip(o.skip[i])?;
        sizes[i] = input.remaining();
    }
    if let Some(id) = inputs[0].identity()
        && inputs[1].identity() == Some(id)
    {
        return Ok(0);
    }
    if let [Some(a), Some(b)] = sizes
        && (null || o.quiet)
        && a != b
        && a.min(b) < o.limit
    {
        return Ok(1);
    }
    if o.limit == 0 {
        for input in &mut inputs {
            input.read(&mut [])?;
        }
    }
    let shared_stdin = o.names == [b"-", b"-"] && sizes.iter().all(Option::is_some);
    let status = |rc| {
        if shared_stdin {
            Err(String::from("-: Bad file descriptor"))
        } else {
            Ok(rc)
        }
    };
    let width = sizes
        .into_iter()
        .flatten()
        .fold(o.limit, u64::min)
        .max(1)
        .ilog10() as usize
        + 1;
    let mut buffers = [vec![0; block], vec![0; block]];
    let mut offset = 0;
    let mut line = 1u64;
    let mut newline = false;
    let mut different = false;
    while offset < o.limit {
        let want = (o.limit - offset).min(block as u64) as usize;
        let [a, b] = &mut buffers;
        let na = fill(&mut inputs[0], &mut a[..want])?;
        let nb = fill(&mut inputs[1], &mut b[..want])?;
        let common = na.min(nb);
        let left = &a[..common];
        let right = &b[..common];
        if left != right {
            different = true;
            if o.quiet || null {
                return status(1);
            }
            if o.list {
                for (i, (&x, &y)) in left
                    .iter()
                    .zip(right)
                    .enumerate()
                    .filter(|(_, (x, y))| x != y)
                {
                    let at = offset + i as u64 + 1;
                    let row = if o.visible {
                        format!("{at:width$} {x:3o} {:4} {y:3o} {}\n", shown(x), shown(y))
                    } else {
                        format!("{at:width$} {x:3o} {y:3o}\n")
                    };
                    out.emit(false, row.as_bytes())?;
                }
            } else {
                let i = left.iter().zip(right).position(|(x, y)| x != y).unwrap();
                line += left[..i].iter().filter(|&&c| c == b'\n').count() as u64;
                let mut msg = Vec::from(o.names[0]);
                msg.push(b' ');
                msg.extend(o.names[1]);
                let word = if o.visible { "byte" } else { byte_word };
                msg.extend(
                    format!(" differ: {word} {}, line {line}", offset + i as u64 + 1).bytes(),
                );
                if o.visible {
                    let (x, y) = (left[i], right[i]);
                    msg.extend(format!(" is {x:3o} {} {y:3o} {}", shown(x), shown(y)).bytes());
                }
                msg.push(b'\n');
                out.emit(false, &msg)?;
                return status(1);
            }
        }
        if !(o.list || o.quiet || null) && common > 0 {
            line += left.iter().filter(|&&b| b == b'\n').count() as u64;
            newline = left.last() == Some(&b'\n');
        }
        offset += common as u64;
        if na != nb {
            if !o.quiet {
                let name = quote(o.names[usize::from(nb < na)]);
                let tail = if offset == 0 {
                    "which is empty".into()
                } else if o.list || null {
                    format!("after byte {offset}")
                } else if newline {
                    format!("after byte {offset}, line {}", line - 1)
                } else {
                    format!("after byte {offset}, in line {line}")
                };
                out.emit(true, format!("cmp: EOF on {name} {tail}\n").as_bytes())?;
            }
            return status(1);
        }
        if common < want {
            break;
        }
    }
    status(u8::from(different))
}
pub fn discard(input: &mut impl Input, mut count: u64) -> Result<(), String> {
    if count > LIMIT {
        return Err("Value too large to be stored in data type".into());
    }
    let mut buf = [0; 512];
    while count > 0 {
        let n = count.min(buf.len() as u64) as usize;
        let n = input.read(&mut buf[..n])?;
        if n == 0 {
            break;
        }
        count -= n as u64;
    }
    Ok(())
}
