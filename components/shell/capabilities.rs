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
            Operand::Edit(_) => return Err(rt::BAD_ARGUMENT),
            Operand::Info(_) | Operand::Noop => return Ok(()),
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
                    if role == w::SELECT_FROM {
                        mutable_leaf(path)?;
                    }
                    let path = super::paths::resolve(fs, path)?;
                    fs.at(w::STAGE, 0, path.as_bytes(), role)?;
                }
                (w::GROUP, 0, alloc::string::String::new())
            }
            Operand::Cwd | Operand::Probe("revocation" | "droits") => {
                (w::CWD_VIEW, 0, alloc::string::String::new())
            }
            Operand::Probe("stat") => {
                fs.simple(w::STAGE, 0)?;
                fs.at(w::STAGE, 0, b".", w::SELECT_LIST)?;
                (w::GROUP, 0, alloc::string::String::new())
            }
            Operand::Probe(
                probe @ ("arbre" | "vide" | "ancien-retrait" | "arbre-revoque" | "vide-revoque"),
            ) => {
                let (scope, name, extra) = match *probe {
                    "arbre" | "arbre-revoque" => (w::REMOVE_TREE, "probe-arbre", w::ENUMERATE),
                    "vide" => (w::REMOVE_EMPTY_DIR, "probe-vide", 0),
                    "vide-revoque" => (w::REMOVE_EMPTY_DIR, "probe-arbre", 0),
                    _ => (w::REMOVE_ENTRY, "temoin", 0),
                };
                let dir = fs.directory(0, b".", w::TRAVERSE | w::REMOVE | extra)?;
                self.handle = Some((dir, true));
                (scope, dir, alloc::string::String::from(name))
            }
            Operand::Probe(_) => return Ok(()),
            Operand::Create(_) | Operand::Remove(_, _) => {
                let (path, scope) = match operand {
                    Operand::Create(path) => (*path, w::CREATE_ENTRY),
                    Operand::Remove(o, nova_cli::command::Program::Rmdir) => {
                        (o.files[0], w::REMOVE_EMPTY_DIR)
                    }
                    Operand::Remove(o, _) if o.has(b'R') || o.has(b'r') => {
                        (o.files[0], w::REMOVE_TREE)
                    }
                    Operand::Remove(o, _) => (o.files[0], w::REMOVE_ENTRY),
                    _ => unreachable!(),
                };
                mutable_leaf(path)?;
                let path = super::paths::resolve(fs, path)?;
                if matches!(operand, Operand::Remove(o, nova_cli::command::Program::Rm) if scope == w::REMOVE_ENTRY && o.has(b'i'))
                {
                    let stat = fs.at(w::LOOKUP_AT, 0, path.as_bytes(), 0)?;
                    if stat[w::PAYLOAD + 24] == 2 {
                        return Err(w::IS_DIR);
                    }
                }
                let (parent, name) = match path.rsplit_once('/') {
                    Some(("", name)) => ("/", name),
                    Some(parts) => parts,
                    None => (".", path.as_str()),
                };
                let needed = match scope {
                    w::CREATE_ENTRY => w::ADD,
                    w::REMOVE_TREE => w::REMOVE | w::ENUMERATE,
                    _ => w::REMOVE,
                };
                let dir = fs.directory(0, parent.as_bytes(), w::TRAVERSE | needed)?;
                self.handle = Some((dir, true));
                (scope, dir, alloc::string::String::from(name))
            }
        };
        self.delegate(fs, scope, handle, leaf.as_bytes())
    }
    pub fn delegate(
        &mut self,
        fs: &mut Client,
        scope: u8,
        handle: u64,
        leaf: &[u8],
    ) -> rt::Result<()> {
        self.channels.0 = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
        self.channels.1 = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
        fs.give(self.channels.0, rt::RECEIVE)?;
        fs.give(self.channels.1, rt::SEND)?;
        fs.delegate(scope, handle, leaf)
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

fn mutable_leaf(path: &str) -> rt::Result<()> {
    if matches!(
        path.trim_end_matches('/').rsplit('/').next(),
        None | Some("" | "." | "..")
    ) {
        Err(rt::DENIED)
    } else {
        Ok(())
    }
}
