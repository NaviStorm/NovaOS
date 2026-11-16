use crate::engine::{Program, Streams};
use alloc::{string::String, vec::Vec};
struct Memory<'a> {
    input: &'a [u8],
    output: Vec<u8>,
}
impl Streams for Memory<'_> {
    fn open(&mut self, _: &[u8], _: bool, _: bool) -> Result<usize, String> {
        Err("no named capability".into())
    }
    fn close(&mut self, _: usize) -> Result<(), String> {
        Ok(())
    }
    fn read(&mut self, _: usize, buf: &mut [u8]) -> Result<usize, String> {
        let n = buf.len().min(self.input.len()).min(3);
        buf[..n].copy_from_slice(&self.input[..n]);
        self.input = &self.input[n..];
        Ok(n)
    }
    fn write(&mut self, _: usize, bytes: &[u8]) -> Result<(), String> {
        self.output.extend(bytes);
        Ok(())
    }
    fn shell(&mut self, _: &[u8]) -> Result<Vec<u8>, String> {
        Err("no shell capability".into())
    }
    fn filename(&self) -> &[u8] {
        b"fixture"
    }
}
pub fn check() -> Result<(), String> {
    let cases: &[(&[u8], i32, &[u8], &[u8], Option<i32>)] = &[
        (
            br"s/^([a-z])([0-9])$/\2\1/p",
            3,
            b"a1\nb2\n",
            b"1a\n2b\n",
            None,
        ),
        (br"N;s/\n/ + /;p", 1, b"un\ndeux\n", b"un + deux\n", None),
        (b"s/introuvable/X/", 0, b"abc\n", b"abc\n", None),
        (b"q42", 0, b"une ligne\nreste\n", b"une ligne\n", Some(42)),
        (b"1p\n$p", 1, b"un\ndeux\ntrois", b"un\ntrois", None),
        (b"h;n;G;p", 1, b"\xff\0\nx\n", b"x\n\xff\0\n", None),
        (b"s/a/z/g", 8, b"a\0\xff\0", b"z\0\xff\0", None),
    ];
    for &(script, flags, input, want, status) in cases {
        let mut io = Memory {
            input,
            output: Vec::new(),
        };
        let actual = Program::compile(script, flags, 70)?.run(&mut io)?;
        if status != actual || want != io.output {
            return Err("unexpected bytes or status".into());
        }
    }
    for (script, flag) in [
        (b"?" as &[u8], 0),
        (b"w denied\ne printf FIXED", 4),
        (b"e printf FIXED", 32),
        (b"s/x/y/w denied", 32),
    ] {
        if Program::compile(script, flag, 70).is_ok() {
            return Err("expected compile-time rejection".into());
        }
    }
    type Vector = (
        &'static str,
        &'static [&'static [u8]],
        &'static [u8],
        &'static [u8],
        i32,
    );
    let vectors: &[Vector] = include!("guest_vectors.in");
    for &(name, args, input, output, code) in vectors {
        let arguments: Vec<_> = args.iter().map(|arg| arg.to_vec()).collect();
        let opts = crate::options::Options::parse(&arguments)?;
        let script = &opts.scripts[0].1;
        let mut io = Memory {
            input,
            output: Vec::new(),
        };
        let mut program =
            Program::compile(script, opts.flags(script) | 32, opts.width.unwrap_or(70))?;
        if program.run(&mut io)?.unwrap_or(0) != code || io.output != output {
            return Err(alloc::format!("vecteur GNU {name}"));
        }
    }
    Ok(())
}
#[cfg(test)]
#[test]
fn portable_cycle() {
    check().unwrap();
}
