use nova_cli::{
    console, fileapi::Client, process, protocol as w, runtime as rt, tee::Streams, tty,
};

pub enum Output {
    Pipe(u64),
    Terminal(alloc::vec::Vec<u8>),
    File { handle: u64, offset: u64 },
}

pub struct Native {
    pub input: u64,
    pub files: rt::Result<Client>,
    pub output: u64,
}

fn terminal(carry: &mut alloc::vec::Vec<u8>, data: &[u8], end: bool) -> rt::Result<()> {
    carry.extend_from_slice(data);
    let mut bytes = carry.as_slice();
    while !bytes.is_empty() {
        match core::str::from_utf8(bytes) {
            Ok(text) => {
                console::printable(tty::OUTPUT, text)?;
                bytes = &[];
            }
            Err(e) => {
                let (good, rest) = bytes.split_at(e.valid_up_to());
                console::printable(tty::OUTPUT, core::str::from_utf8(good).unwrap())?;
                if e.error_len().is_none() && !end {
                    bytes = rest;
                    break;
                }
                console::write(tty::OUTPUT, &alloc::format!("\\x{:02x}", rest[0]))?;
                bytes = &rest[1..];
            }
        }
    }
    let used = carry.len() - bytes.len();
    carry.drain(..used);
    Ok(())
}
impl Native {
    fn client(&mut self) -> rt::Result<&mut Client> {
        self.files.as_mut().map_err(|e| *e)
    }
    pub fn open_selected(&mut self, slot: u64) -> rt::Result<Output> {
        let c = self.client()?;
        let response = c.simple(w::CREATE_NEW, slot)?;
        let handle = w::word(&response, 16);
        // Le nom devient visible dès l’ouverture, même pour une entrée vide.
        if let Err(e) = c.simple(w::PUBLISH_COPY, slot) {
            let _ = c.close(handle);
            return Err(e);
        }
        Ok(Output::File { handle, offset: 0 })
    }
}
impl Streams for Native {
    type Sink = Output;
    type Error = u64;
    fn stdout(&mut self) -> Output {
        if self.output == 0 {
            Output::Terminal(alloc::vec::Vec::new())
        } else {
            Output::Pipe(self.output)
        }
    }
    fn open(&mut self, index: usize, _: &[u8], _: bool) -> rt::Result<Output> {
        self.open_selected(index as u64 + 1)
    }
    fn read(&mut self, buf: &mut [u8]) -> rt::Result<usize> {
        process::pipe_read(self.input, buf)
    }
    fn write(&mut self, sink: &mut Output, bytes: &[u8]) -> rt::Result<()> {
        match sink {
            Output::Pipe(port) => process::pipe_write(*port, bytes)
                .map(|_| ())
                .map_err(|(_, code)| code),
            Output::Terminal(carry) => terminal(carry, bytes, false),
            Output::File { handle, offset } => {
                let c = self.client()?;
                c.write(*handle, *offset, bytes)?;
                c.sync(*handle)?;
                *offset += bytes.len() as u64;
                Ok(())
            }
        }
    }
    fn close(&mut self, sink: Output) -> rt::Result<()> {
        match sink {
            Output::Terminal(mut carry) => terminal(&mut carry, &[], true),
            Output::Pipe(port) => rt::value(rt::call(rt::IPC_CLOSE, port, 0, 0)).map(|_| ()),
            Output::File { handle, .. } => self.client()?.close(handle),
        }
    }
    fn report(&mut self, name: &[u8], code: u64) {
        let _ = console::printable(
            tty::ERROR,
            &alloc::format!(
                "tee: {}: {}\n",
                alloc::string::String::from_utf8_lossy(name),
                console::reason(code)
            ),
        );
    }
    fn broken(&self, sink: &Output, code: u64) -> bool {
        matches!(sink, Output::Pipe(_)) && code == rt::CLOSED
    }
    fn default_pipe_status(&self) -> i32 {
        rt::CLOSED as i32
    }
}
