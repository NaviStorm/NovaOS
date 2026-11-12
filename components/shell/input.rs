//! Les positions gauche/droite sont aussi accessibles par Ctrl-B/Ctrl-F.
use nova_cli::{runtime as rt, tty};
pub enum Key {
    Home,
    End,
    Up,
    Down,
    Delete,
    Left,
    Right,
    Erase,
    Tab,
    Enter,
    Char(char),
    Invalid,
    Ignore,
}
fn byte() -> rt::Result<u8> {
    Ok(rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))? as u8)
}
#[derive(Default)]
pub struct Input {
    skip_lf: bool,
    pending: Option<u8>,
}
impl Input {
    pub fn next(&mut self) -> rt::Result<Key> {
        let b = match self.pending.take() {
            Some(b) => b,
            None => byte()?,
        };
        if core::mem::take(&mut self.skip_lf) && b == 10 {
            return Ok(Key::Ignore);
        }
        Ok(match b {
            10 | 13 => {
                self.skip_lf = b == 13;
                Key::Enter
            }
            9 => Key::Tab,
            8 | 127 => Key::Erase,
            1 => Key::Home,
            5 => Key::End,
            2 => Key::Left,
            6 => Key::Right,
            27 => {
                let prefix = byte()?;
                if !matches!(prefix, b'[' | b'O') {
                    self.pending = Some(prefix);
                    Key::Invalid
                } else {
                    match byte()? {
                        b'A' => Key::Up,
                        b'B' => Key::Down,
                        b'H' => Key::Home,
                        b'F' => Key::End,
                        action @ (b'1' | b'3' | b'4' | b'7' | b'8') => {
                            let term = byte()?;
                            if term != b'~' {
                                self.pending = Some(term);
                                Key::Invalid
                            } else {
                                match action {
                                    b'3' => Key::Delete,
                                    b'1' | b'7' => Key::Home,
                                    _ => Key::End,
                                }
                            }
                        }
                        b'D' => Key::Left,
                        b'C' => Key::Right,
                        c => {
                            if matches!(c, 10 | 13) {
                                self.pending = Some(c)
                            }
                            Key::Invalid
                        }
                    }
                }
            }
            b' '..=126 => Key::Char(b as char),
            _ => {
                let width = match b {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Ok(Key::Invalid),
                };
                let mut bytes = [b; 4];
                for slot in &mut bytes[1..width] {
                    let next = byte()?;
                    if next & 0xc0 != 0x80 {
                        self.pending = Some(next);
                        return Ok(Key::Invalid);
                    }
                    *slot = next;
                }
                core::str::from_utf8(&bytes[..width])
                    .ok()
                    .and_then(|s| s.chars().next())
                    .filter(|c| nova_cli::console::safe(*c))
                    .map(Key::Char)
                    .unwrap_or(Key::Invalid)
            }
        })
    }
}
