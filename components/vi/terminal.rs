//! Décodage du clavier série et boucle de présentation.
use super::{
    editor::{Editor, Mode},
    screen::Screen,
};
use nova_cli::{runtime as rt, vi_wire as wire};

pub struct Keys {
    held: Option<u8>,
}
impl Keys {
    pub fn new() -> Self {
        Self { held: None }
    }
    fn byte(&mut self, link: &mut wire::Link) -> rt::Result<u8> {
        if let Some(b) = self.held.take() {
            return Ok(b);
        }
        let r = link.ask(wire::KEY, &[])?;
        if r.len != 2 {
            return Err(rt::BAD_ARGUMENT);
        }
        Ok(r.payload[1])
    }
    pub fn next(&mut self, link: &mut wire::Link) -> rt::Result<Option<char>> {
        if self.held.is_none() {
            let reply = link.ask(wire::KEY, &[1])?;
            match reply.len {
                1 => return Ok(None),
                2 => self.held = Some(reply.payload[1]),
                _ => return Err(rt::BAD_ARGUMENT),
            }
        }
        let b = self.byte(link)?;
        match b {
            // Le courtier reconnaît déjà CRLF, y compris au passage du dialogue OUI.
            13 => Ok(Some('\n')),
            127 => Ok(Some('\x08')),
            0..=127 => Ok(Some(b as char)),
            _ => {
                let len = match b {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Ok(Some('\0')),
                };
                let mut buf = [b; 4];
                for slot in &mut buf[1..len] {
                    let next = self.byte(link)?;
                    if next & 0xc0 != 0x80 {
                        self.held = Some(next);
                        return Ok(Some('\0'));
                    }
                    *slot = next;
                }
                Ok(Some(
                    core::str::from_utf8(&buf[..len])
                        .ok()
                        .and_then(|s| s.chars().next())
                        .unwrap_or('\0'),
                ))
            }
        }
    }
}

#[derive(Clone, Copy)]
struct BeforeEsc {
    mode: Mode,
    at: usize,
}
pub fn edit(
    e: &mut Editor,
    link: &mut wire::Link,
    tail: &mut super::paging::Tail,
) -> rt::Result<()> {
    let mut keys = Keys::new();
    let mut screen = Screen::default();
    screen.geometry(link, true)?;
    let mut escaped = None;
    let mut ss3: Option<BeforeEsc> = None;
    let mut sequence: Option<(BeforeEsc, bool)> = None;
    screen.draw(e, link)?;
    loop {
        let Some(key) = keys.next(link)? else {
            screen.geometry(link, false)?;
            screen.draw(e, link)?;
            continue;
        };
        if key == '\x0c' {
            screen.geometry(link, true)?;
            screen.draw(e, link)?;
            continue;
        }
        let submitting = e.mode == Mode::Command && key == '\n';
        if let Some(before) = ss3.take() {
            if matches!(key, 'A' | 'B' | 'C' | 'D') {
                e.mode = before.mode;
                e.buf.at = before.at;
                if let Err(msg) = tail.before(e, key, true, link) {
                    e.message = msg;
                } else {
                    e.arrow(key);
                }
                screen.draw(e, link)?;
                continue;
            }
            // Esc suivi de la commande normale O : le prochain caractère lève l'ambiguïté SS3.
            if let Err(msg) = tail.before(e, 'O', false, link) {
                e.message = msg;
                screen.draw(e, link)?;
                continue;
            }
            e.key('O', link);
        }
        if key == '\x1b' {
            sequence = None;
            escaped = Some(BeforeEsc {
                mode: e.mode,
                at: e.buf.at,
            });
            e.escape();
        } else if let Some((before, plain)) = sequence {
            if ('@'..='~').contains(&key) {
                sequence = None;
                e.mode = before.mode;
                e.buf.at = before.at;
                if plain && matches!(key, 'A' | 'B' | 'C' | 'D') {
                    if let Err(msg) = tail.before(e, key, true, link) {
                        e.message = msg;
                    } else {
                        e.arrow(key);
                    }
                } else {
                    e.message = "séquence clavier refusée";
                }
            } else if (' '..='?').contains(&key) {
                sequence = Some((before, false));
                continue;
            } else {
                sequence = None;
                e.message = "séquence clavier refusée";
            }
        } else if let Some(before) = escaped.take()
            && matches!(key, '[' | 'O')
        {
            if key == 'O' {
                ss3 = Some(before);
            } else {
                sequence = Some((before, true));
            }
            continue;
        } else if let Err(msg) = tail.before(e, key, false, link) {
            if submitting || e.mode == Mode::Search && key == '\n' {
                e.mode = Mode::Normal;
            }
            e.message = msg;
        } else if e.key(key, link) {
            return Ok(());
        }
        if submitting {
            screen.geometry(link, false)?;
        }
        screen.draw(e, link)?;
    }
}
