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
            Operand::Cwd | Operand::Probe("revocation" | "droits") => (w::CWD_VIEW, 0, ""),
            Operand::Probe(_) => return Ok(()),
            Operand::Directory(path, _) => {
                let dir = fs.directory(0, path.as_bytes(), w::TRAVERSE | w::ENUMERATE)?;
                self.handle = Some((dir, true));
                (w::DIR_VIEW, dir, "")
            }
            Operand::Read(path, _) => {
                let file = fs.open_at(0, path.as_bytes(), w::READABLE)?;
                self.handle = Some((file, false));
                (w::FILE_VIEW, file, "")
            }
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
