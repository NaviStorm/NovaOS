use super::buffer::Buffer;
use alloc::string::String;
use nova_cli::{console, protocol as w, runtime as rt};

pub const PROMPT_MAX: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Search,
}
pub struct Editor {
    pub buf: Buffer,
    pub name: String,
    pub exists: bool,
    pub mode: Mode,
    pub prompt: String,
    pub message: &'static str,
    last_search: String,
    pending: Option<char>,
}

pub trait Storage {
    fn publish(&mut self, name: &str, text: &[u8]) -> rt::Result<()>;
}

impl Editor {
    pub fn new(name: &str, text: &str, exists: bool) -> Result<Self, &'static str> {
        Ok(Self {
            buf: Buffer::new(text)?,
            name: name.into(),
            exists,
            mode: Mode::Normal,
            prompt: String::new(),
            last_search: String::new(),
            pending: None,
            message: "",
        })
    }
    pub fn escape(&mut self) {
        self.pending = None;
        if self.mode == Mode::Insert {
            self.buf.left();
        }
        self.mode = Mode::Normal;
        self.buf.normal();
    }
    pub fn arrow(&mut self, key: char) {
        self.pending = None;
        if !matches!(self.mode, Mode::Normal | Mode::Insert) {
            self.message = "flèche indisponible dans la commande";
            return;
        }
        let inserting = self.mode == Mode::Insert;
        match key {
            'A' => self.buf.vertical(false, inserting),
            'B' => self.buf.vertical(true, inserting),
            'C' => self.buf.right(inserting),
            'D' => self.buf.left(),
            _ => (),
        }
    }
    fn find(&mut self) {
        self.message = if self.buf.search(&self.last_search) {
            "trouvé"
        } else {
            "mot introuvable"
        };
    }
    fn write(&mut self, other: Option<&str>, io: &mut impl Storage) -> bool {
        let name = other.unwrap_or(&self.name);
        if self.exists && name == self.name {
            self.message = "fichier existant : remplacement refusé";
            return false;
        }
        match io.publish(name, self.buf.text.as_bytes()) {
            Ok(()) => {
                if let Some(name) = other {
                    self.name = name.into();
                }
                self.exists = true;
                self.buf.saved();
                self.message = "fichier enregistré";
                true
            }
            Err(e) => {
                self.message = match e {
                    w::EXISTS => "fichier existant : remplacement refusé",
                    rt::DENIED => "destination non autorisée",
                    w::UNCERTAIN => "publication incertaine : vérifier le disque",
                    _ => console::reason(e),
                };
                false
            }
        }
    }
    fn submit(&mut self, io: &mut impl Storage) -> bool {
        let search = self.mode == Mode::Search;
        self.mode = Mode::Normal;
        let prompt = core::mem::take(&mut self.prompt);
        if search {
            if !prompt.is_empty() {
                self.last_search = prompt;
            }
            self.find();
            return false;
        }
        match prompt.as_str() {
            "q!" => return true,
            "q" if !self.buf.dirty() => return true,
            "q" => self.message = "modifications non sauvegardées",
            "x" if self.exists && !self.buf.dirty() => return true,
            "x" | "wq" => return self.write(None, io),
            "w" => {
                self.write(None, io);
            }
            command if command.starts_with("w ") && command.len() > 2 => {
                self.write(Some(&command[2..]), io);
            }
            _ => self.message = "commande non prise en charge",
        }
        false
    }
    pub fn key(&mut self, key: char, io: &mut impl Storage) -> bool {
        self.message = "";
        if matches!(self.mode, Mode::Command | Mode::Search) {
            match key {
                '\n' => return self.submit(io),
                '\x08' => {
                    self.prompt.pop();
                }
                c if console::safe(c) && self.prompt.len() + c.len_utf8() <= PROMPT_MAX => {
                    self.prompt.push(c)
                }
                _ => self.message = "caractère ou longueur refusés",
            }
            return false;
        }
        let result = if self.mode == Mode::Insert {
            match key {
                '\x08' => self.buf.erase(),
                c if c == '\n' || c == '\t' || console::safe(c) => self.buf.insert(c),
                _ => Err("caractère refusé"),
            }
        } else if let Some(prefix) = self.pending.take() {
            match (prefix, key) {
                ('g', 'g') => {
                    self.buf.at = 0;
                    Ok(())
                }
                ('d', 'd') => {
                    self.buf.change();
                    self.buf.delete_line()
                }
                _ => Err("commande non prise en charge"),
            }
        } else {
            match key {
                'h' => {
                    self.buf.left();
                    Ok(())
                }
                'l' => {
                    self.buf.right(false);
                    Ok(())
                }
                'j' | 'k' => {
                    self.buf.vertical(key == 'j', false);
                    Ok(())
                }
                '0' => {
                    self.buf.at = self.buf.start();
                    Ok(())
                }
                '$' => {
                    self.buf.at = self.buf.end();
                    self.buf.normal();
                    Ok(())
                }
                'G' => {
                    self.buf.last();
                    Ok(())
                }
                'd' | 'g' => {
                    self.pending = Some(key);
                    Ok(())
                }
                'i' | 'a' | 'o' | 'O' => {
                    self.buf.change();
                    let result = match key {
                        'a' => {
                            self.buf.right(true);
                            Ok(())
                        }
                        'o' | 'O' => self.buf.open_line(key == 'o'),
                        _ => Ok(()),
                    };
                    if result.is_ok() {
                        self.mode = Mode::Insert;
                    }
                    result
                }
                'x' => {
                    self.buf.change();
                    self.buf.delete()
                }
                'u' if self.buf.undo() => Ok(()),
                'u' => Err("aucune annulation"),
                '/' | ':' => {
                    self.prompt.clear();
                    self.mode = if key == '/' {
                        Mode::Search
                    } else {
                        Mode::Command
                    };
                    Ok(())
                }
                'n' => {
                    self.find();
                    Ok(())
                }
                _ => Err("commande non prise en charge"),
            }
        };
        if let Err(error) = result {
            self.message = error;
        }
        false
    }
}
