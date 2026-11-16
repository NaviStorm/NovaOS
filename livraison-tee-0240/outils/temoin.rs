//! Greffé uniquement au profil de recette de cp dans une copie de construction privée.
//! La délégation ordinaire fournit la source 1 en lecture et le nom libre 2.
use nova_cli::{
    console,
    fileapi::Client,
    protocol as w, runtime as rt,
    tee::{self, Action, Mode, Streams},
    tty,
};
#[path = "tee/native_io.rs"]
mod native_io;
use native_io::{Native, Output};

enum Destination {
    Capture(usize),
    P9(Output),
}
struct Bench {
    io: Native,
    offset: u64,
    source: alloc::vec::Vec<u8>,
    copies: [alloc::vec::Vec<u8>; 2],
    errors: usize,
    break_stdout: bool,
    fail_file: bool,
}
impl Streams for Bench {
    type Sink = Destination;
    type Error = u64;
    fn stdout(&mut self) -> Destination {
        Destination::Capture(0)
    }
    fn open(&mut self, _: usize, name: &[u8], _: bool) -> rt::Result<Destination> {
        match name {
            b"p9" => self.io.open_selected(2).map(Destination::P9),
            b"bad" => Err(rt::DENIED),
            _ => Ok(Destination::Capture(1)),
        }
    }
    fn read(&mut self, buf: &mut [u8]) -> rt::Result<usize> {
        let start = self.offset as usize;
        let n = (self.source.len() - start).min(buf.len());
        buf[..n].copy_from_slice(&self.source[start..start + n]);
        self.offset += n as u64;
        Ok(n)
    }
    fn write(&mut self, s: &mut Destination, data: &[u8]) -> rt::Result<()> {
        match s {
            Destination::Capture(0) if self.break_stdout => Err(rt::CLOSED),
            Destination::Capture(1) if self.fail_file => Err(w::IO),
            Destination::Capture(i) => {
                self.copies[*i].extend_from_slice(data);
                Ok(())
            }
            Destination::P9(o) => self.io.write(o, data),
        }
    }
    fn close(&mut self, s: Destination) -> rt::Result<()> {
        match s {
            Destination::P9(o) => self.io.close(o),
            _ => Ok(()),
        }
    }
    fn broken(&self, s: &Destination, code: u64) -> bool {
        matches!(s, Destination::Capture(0)) && code == rt::CLOSED
    }
    fn report(&mut self, _: &[u8], _: u64) {
        self.errors += 1;
    }
    fn default_pipe_status(&self) -> i32 {
        rt::CLOSED as i32
    }
}
pub fn run(c: Client, dest: &str) -> rt::Result<()> {
    let occupied = dest == "occupe";
    let mut io = Native {
        input: 0,
        output: 0,
        files: Ok(c),
    };
    let c = io.files.as_mut().unwrap();
    let size = w::word(&c.simple(w::STAT, 1)?, w::PAYLOAD + 16) as usize;
    if size > 8192 {
        return Err(rt::LIMIT);
    }
    let mut source = alloc::vec![0;size];
    let mut start = 0;
    while start < size {
        let n = c.read(1, start as u64, &mut source[start..])?;
        if n == 0 {
            return Err(w::IO);
        }
        start += n;
    }
    let mut b = Bench {
        io,
        source,
        offset: 0,
        copies: Default::default(),
        errors: 0,
        break_stdout: false,
        fail_file: false,
    };
    let mut buf = [0; 128];
    if matches!(dest, "mesure1" | "mesure2") {
        let files: &[&[u8]] = if dest == "mesure1" { &[] } else { &[b"p9"] };
        let Ok(Action::Copy(o)) = tee::parse(files, false) else {
            return Err(3);
        };
        console::write(tty::OUTPUT, "TEE-DEBUT\n")?;
        let rc = tee::copy(&o, &mut b, &mut buf);
        console::write(tty::OUTPUT, "TEE-FIN\n")?;
        if rc != 0 || b.copies[0] != b.source {
            return Err(w::IO);
        }
        return Ok(());
    }
    let forms: &[(&[u8], Mode)] = &[
        (b"", Mode::Default),
        (b"-p", Mode::WarnNoPipe),
        (b"--out", Mode::WarnNoPipe),
        (b"--output-error=warn", Mode::Warn),
        (b"--output-error=warn-nopipe", Mode::WarnNoPipe),
        (b"--output-error=exit", Mode::Exit),
        (b"--output-error=exit-nopipe", Mode::ExitNoPipe),
    ];
    let mut count = 0;
    for &(flag, mode) in forms {
        for scenario in 0..4 {
            let mut args: alloc::vec::Vec<&[u8]> = alloc::vec![b"-a", b"--ignore-interrupts"];
            if !flag.is_empty() {
                args.push(flag);
            }
            if scenario == 3 {
                args.push(b"bad");
            }
            args.push(b"capture");
            let Ok(Action::Copy(opts)) = tee::parse(&args, false) else {
                return Err(3);
            };
            if opts.mode != mode || !opts.append || !opts.ignore_interrupts {
                return Err(3);
            }
            b.offset = 0;
            b.errors = 0;
            b.copies = Default::default();
            b.break_stdout = scenario == 1;
            b.fail_file = scenario == 2;
            let early = matches!(mode, Mode::Exit | Mode::ExitNoPipe);
            let nonempty = !b.source.is_empty();
            let pipe_error = scenario == 1 && nonempty;
            let abort_pipe = pipe_error && matches!(mode, Mode::Default | Mode::Exit);
            let abort_open = scenario == 3 && early;
            let diagnostic = (pipe_error && matches!(mode, Mode::Warn | Mode::Exit))
                || (scenario == 2 && nonempty)
                || scenario == 3;
            let expected = if pipe_error && mode == Mode::Default {
                rt::CLOSED as i32
            } else {
                i32::from(diagnostic)
            };
            let status = tee::copy(&opts, &mut b, &mut buf);
            let stdout = if abort_open || pipe_error {
                &[][..]
            } else if scenario == 2 && early {
                &b.source[..size.min(buf.len())]
            } else {
                &b.source
            };
            let extra = if abort_pipe || abort_open || scenario == 2 {
                &[][..]
            } else {
                &b.source
            };
            if status != expected
                || b.errors != usize::from(diagnostic)
                || b.copies[0] != stdout
                || b.copies[1] != extra
            {
                return Err(w::IO);
            }
            count += 1;
        }
    }
    for (words, valid) in [
        (b"--append".as_slice(), true),
        (b"-i", true),
        (b"--output-error=x", false),
        (b"--output-error=w", false),
        (b"-h", false),
    ] {
        if tee::parse(&[words], false).is_ok() != valid {
            return Err(3);
        }
        count += 1;
    }
    b.offset = 0;
    b.errors = 0;
    b.copies = Default::default();
    b.break_stdout = false;
    b.fail_file = false;
    let args: &[&[u8]] = if occupied {
        &[b"--append", b"p9", b"capture"]
    } else {
        &[b"-i", b"p9", b"capture"]
    };
    let Ok(Action::Copy(o)) = tee::parse(args, false) else {
        return Err(3);
    };
    let rc = tee::copy(&o, &mut b, &mut buf);
    if rc != i32::from(occupied)
        || b.errors != usize::from(occupied)
        || b.copies.iter().any(|v| v != &b.source)
    {
        return Err(w::IO);
    }
    console::write(
        tty::OUTPUT,
        &alloc::format!("TEE-LAB matrices={count} p9={rc} stdout={size} miroir={size}\n"),
    )
}
