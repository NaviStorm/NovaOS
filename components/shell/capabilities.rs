use nova_cli::{command::Operand, fileapi::Client, protocol as w, runtime as rt};
pub struct Grant {
    pub channels: (u64, u64),
    handle: Option<(u64, bool)>,
}
impl Grant {
    pub fn new() -> Self {
        Self {
            channels: (0, 0),
            handle: None,
        }
    }
    pub fn select(&mut self, fs: &mut Client, operand: &Operand) -> rt::Result<()> {
        let (scope, handle, leaf) = match operand {
            Operand::Info(_) => return Ok(()),
            Operand::Batch(opts, program) => {
                use nova_cli::command::Program::*;
                fs.simple(w::STAGE, 0)?;
                let default = ["."];
                let paths = if opts.files.is_empty() {
                    &default[..]
                } else {
                    &opts.files
                };
                for (i, path) in paths.iter().enumerate() {
                    let role = match program {
                        Ls if opts.has(b'R') => w::SELECT_TREE,
                        Ls => w::SELECT_LIST,
                        Cp if i == 0 && (opts.has(b'R') || opts.has(b'r')) => w::SELECT_COPY_TREE,
                        Cp if i == 0 => w::SELECT_FILE,
                        Cp if opts.has(b'R') || opts.has(b'r') => w::SELECT_COPY_DEST,
                        Cp => w::SELECT_NEW,
                        Mv if i == 0 => w::SELECT_FROM,
                        Mv => w::SELECT_TO,
                        Touch => w::SELECT_TOUCH,
                        Cat | Head | Tail | Wc | More => w::SELECT_FILE,
                        _ => return Err(rt::BAD_ARGUMENT),
                    };
                    fs.at(w::STAGE, 0, path.as_bytes(), role)?;
                }
                (w::GROUP, 0, "")
            }
            Operand::Cwd | Operand::Probe("revocation" | "droits") => (w::CWD_VIEW, 0, ""),
            Operand::Probe("stat") => {
                fs.simple(w::STAGE, 0)?;
                fs.at(w::STAGE, 0, b".", w::SELECT_LIST)?;
                (w::GROUP, 0, "")
            }
            Operand::Probe(_) => return Ok(()),
            Operand::Create(path) | Operand::Remove(path) => {
                if path.len() > w::PATH_MAX || path.contains("//") {
                    return Err(rt::BAD_ARGUMENT);
                }
                let (parent, name) = match path.rsplit_once('/') {
                    Some(("", name)) => ("/", name),
                    Some(parts) => parts,
                    None => (".", *path),
                };
                if name.is_empty() || matches!(name, "." | "..") {
                    return Err(rt::DENIED);
                }
                let create = matches!(operand, Operand::Create(_));
                let dir = fs.directory(
                    0,
                    parent.as_bytes(),
                    w::TRAVERSE | if create { w::ADD } else { w::REMOVE },
                )?;
                self.handle = Some((dir, true));
                (
                    if create {
                        w::CREATE_ENTRY
                    } else {
                        w::REMOVE_ENTRY
                    },
                    dir,
                    name,
                )
            }
        };
        self.channels.0 = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
        self.channels.1 = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
        fs.give(self.channels.0, rt::RECEIVE)?;
        fs.give(self.channels.1, rt::SEND)?;
        fs.delegate(scope, handle, leaf.as_bytes())
    }
    pub fn release(&mut self, fs: &mut Client) -> rt::Result<()> {
        // Détruire les racines réveille le service, y compris après une faute du fils.
        let mut outcome = Ok(());
        for h in [self.channels.0, self.channels.1] {
            if h != 0 {
                outcome = outcome.and(rt::value(rt::call(rt::IPC_CLOSE, h, 0, 0)).map(|_| ()));
            }
        }
        self.channels = (0, 0);
        if let Some((h, dir)) = self.handle.take() {
            outcome = outcome.and(
                fs.simple(if dir { w::DIR_CLOSE } else { w::CLOSE }, h)
                    .map(|_| ()),
            )
        }
        outcome
    }
}
