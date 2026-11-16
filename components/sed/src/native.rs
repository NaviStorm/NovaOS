use crate::{
    engine::{Program, Streams},
    options::Options,
};
use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};
use nova_cli::{console, fileapi::Client, process, protocol as fs, runtime as rt, tty};

enum Input {
    File {
        client: Client,
        offset: u64,
        end: u64,
    },
    Pipe {
        key: u64,
        frame: [u8; process::PIPE_FRAME],
        start: usize,
        end: usize,
    },
    Terminal,
}
struct Native<'a> {
    input: Input,
    producer: Option<u64>,
    name: &'a [u8],
    failure: u64,
    carry: Vec<u8>,
}
impl Native<'_> {
    fn check<T>(&mut self, result: rt::Result<T>) -> Result<T, String> {
        result.map_err(|code| {
            self.failure = code;
            console::reason(code).to_string()
        })
    }
    fn display(&mut self, bytes: &[u8], eof: bool) -> rt::Result<()> {
        self.carry.extend_from_slice(bytes);
        let mut data = self.carry.as_slice();
        loop {
            match core::str::from_utf8(data) {
                Ok(text) => {
                    console::printable(tty::OUTPUT, text)?;
                    self.carry.clear();
                    return Ok(());
                }
                Err(e) => {
                    let n = e.valid_up_to();
                    console::printable(tty::OUTPUT, core::str::from_utf8(&data[..n]).unwrap())?;
                    data = &data[n..];
                    if !eof && e.error_len().is_none() {
                        let tail = data.len();
                        let start = self.carry.len() - tail;
                        self.carry.copy_within(start.., 0);
                        self.carry.truncate(tail);
                        return Ok(());
                    }
                    console::write(tty::OUTPUT, &format!("\\x{:02x}", data[0]))?;
                    data = &data[1..];
                }
            }
        }
    }
}
impl Streams for Native<'_> {
    fn open(&mut self, _: &[u8], _: bool, _: bool) -> Result<usize, String> {
        Err("capacité de fichier nommé absente".into())
    }
    fn close(&mut self, _: usize) -> Result<(), String> {
        Ok(())
    }
    fn read(&mut self, _: usize, buf: &mut [u8]) -> Result<usize, String> {
        let result = match &mut self.input {
            Input::File {
                client,
                offset,
                end,
            } => {
                let max = (*end - *offset).min(640).min(buf.len() as u64) as usize;
                if max == 0 {
                    return Ok(0);
                }
                client.read(1, *offset, &mut buf[..max]).and_then(|n| {
                    if n == 0 || n > max {
                        Err(fs::IO)
                    } else {
                        *offset += n as u64;
                        Ok(n)
                    }
                })
            }
            Input::Pipe {
                key,
                frame,
                start,
                end,
            } => {
                if *start == *end {
                    *end = match process::pipe_read(*key, frame) {
                        Ok(count) => count,
                        Err(code) => {
                            self.failure = code;
                            return Err(console::reason(code).into());
                        }
                    };
                    *start = 0;
                }
                let count = buf.len().min(*end - *start);
                buf[..count].copy_from_slice(&frame[*start..*start + count]);
                *start += count;
                Ok(count)
            }
            Input::Terminal => {
                rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0)).map(|v| {
                    let b = v as u8;
                    if b == 4 {
                        0
                    } else {
                        buf[0] = if b == 13 { 10 } else { b };
                        1
                    }
                })
            }
        };
        self.check(result)
    }
    fn write(&mut self, _: usize, bytes: &[u8]) -> Result<(), String> {
        let result = if let Some(key) = self.producer {
            process::pipe_write(key, bytes)
                .map(|_| ())
                .map_err(|(_, code)| code)
        } else {
            self.display(bytes, false)
        };
        self.check(result)
    }
    fn shell(&mut self, _: &[u8]) -> Result<Vec<u8>, String> {
        Err("NovaOS : exécution de shell indisponible".into())
    }
    fn filename(&self) -> &[u8] {
        self.name
    }
}
fn command() -> Result<u64, (u64, String)> {
    let map = |code| (code, console::reason(code).to_string());
    let args = process::arguments().map_err(map)?;
    let words = args
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect::<Vec<_>>();
    let opt = Options::parse(&words).map_err(|e| (1, e))?;
    if opt.help || opt.version {
        let fr = nova_cli::documentation::child_language(&args)
            == nova_cli::documentation::Language::French;
        let text = if opt.help {
            if fr { crate::HELP_FR } else { crate::HELP_EN }
        } else {
            crate::VERSION
        };
        console::write(tty::OUTPUT, text).map_err(map)?;
        return Ok(0);
    }
    if opt.inplace.is_some() {
        return Err((
            4,
            "-i : P9 ne peut pas remplacer une destination existante".into(),
        ));
    }
    if opt.debug {
        return Err((1, "--debug indisponible".into()));
    }
    if opt.files.len() > 1 || opt.scripts.iter().any(|(file, _)| *file) {
        return Err((
            1,
            "les opérandes multiples et -f exigent une délégation supplémentaire".into(),
        ));
    }
    let mut script = Vec::new();
    for (_, s) in &opt.scripts {
        if !script.is_empty() && !script.ends_with(b"\n") {
            script.push(b'\n');
        }
        script.extend(s);
    }
    let mut program = Program::compile(&script, opt.flags(&script) | 32, opt.width.unwrap_or(70))
        .map_err(|e| (1, e))?;
    let status = rt::config(3);
    if status != 0 {
        return Err(map(status));
    }
    let writer = rt::config(1);
    let reader = rt::config(2);
    let name = opt.files.first().map(Vec::as_slice).unwrap_or(b"-");
    let input = if name != b"-" {
        let mut client = Client::scoped().map_err(map)?;
        let end = fs::word(&client.simple(fs::STAT, 1).map_err(map)?, fs::PAYLOAD + 16);
        Input::File {
            client,
            offset: 0,
            end,
        }
    } else if reader != 0 {
        Input::Pipe {
            key: reader,
            frame: [0; process::PIPE_FRAME],
            start: 0,
            end: 0,
        }
    } else {
        Input::Terminal
    };
    let mut io = Native {
        input,
        name,
        producer: (writer != 0).then_some(writer),
        failure: 4,
        carry: Vec::new(),
    };
    let status = program.run(&mut io).map_err(|e| (io.failure, e))?;
    if reader != 0 {
        rt::value(rt::call(rt::IPC_CLOSE, reader, 0, 0)).map_err(map)?;
    }
    if writer == 0 {
        io.display(&[], true).map_err(map)?;
    }
    Ok(status.unwrap_or(0) as u64)
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    #[cfg(feature = "qualification")]
    if rt::config(4) == 0 {
        // Le profil SDK lance ce témoin sans argument ni capacité métier.
        rt::exit(if crate::fixtures::check().is_ok() {
            0
        } else {
            1
        });
    }
    match command() {
        Ok(n) => rt::exit(n),
        Err((code, why)) => {
            let _ = console::printable(tty::ERROR, &format!("sed : {why}\n"));
            rt::exit(code)
        }
    }
}
