//! Fenêtre série imprimable. Le flux délégué n'accepte aucune séquence ANSI.
use super::editor::{Editor, Mode};
use alloc::string::String;
use core::fmt::Write;
use nova_cli::{console, runtime as rt, tty, vi_wire as wire};
use unicode_width::UnicodeWidthChar;

const WIDTH: usize = 56;
const ROWS: usize = 5;
fn width(c: char) -> usize {
    if c == '\t' {
        4
    } else if console::safe(c) {
        c.width().unwrap_or(1).max(1)
    } else {
        1
    }
}
fn display(out: &mut String, c: char) {
    if c == '\t' {
        out.push_str("    ")
    } else if console::safe(c) && c.width() != Some(0) {
        out.push(c)
    } else {
        out.push('�')
    }
}
pub fn draw(e: &Editor) -> rt::Result<()> {
    let row = e.buf.row();
    let first = row.saturating_sub(ROWS / 2);
    let start = e.buf.start();
    let pos = e.buf.text[start..e.buf.at]
        .chars()
        .map(width)
        .sum::<usize>();
    let left = pos.saturating_sub(WIDTH - 8);
    let mut out = String::new();
    out.push('\n');
    for (j, line) in e.buf.text.split('\n').skip(first).take(ROWS).enumerate() {
        let current = first + j == row;
        let line = line.strip_suffix('\r').unwrap_or(line);
        let _ = write!(out, "{:>3} ", first + j + 1);
        let mut col = 0;
        for (i, c) in line
            .char_indices()
            .chain(core::iter::once((line.len(), '\0')))
        {
            if col >= left && col < left + WIDTH {
                if current && i == e.buf.at - start {
                    out.push('▏');
                }
                if c != '\0' {
                    display(&mut out, c);
                }
            }
            if c == '\0' {
                break;
            }
            col += width(c);
            if col >= left + WIDTH {
                break;
            }
        }
        out.push('\n');
    }
    let mode = match e.mode {
        Mode::Insert => "INSERTION",
        Mode::Normal => "NORMAL",
        Mode::Command => "COMMANDE",
        Mode::Search => "RECHERCHE",
    };
    let status = if e.buf.dirty() {
        "modifié"
    } else if e.exists {
        "enregistré"
    } else {
        "nouveau"
    };
    let _ = writeln!(
        out,
        "vi [{mode}] {} [{status}] {}:{}",
        e.name,
        row + 1,
        e.buf.col() + 1
    );
    if !e.message.is_empty() {
        let _ = writeln!(out, "{}", e.message);
    }
    if matches!(e.mode, Mode::Command | Mode::Search) {
        let _ = writeln!(
            out,
            "{}{}▏",
            if e.mode == Mode::Search { '/' } else { ':' },
            e.prompt
        );
    }
    console::printable(tty::OUTPUT, &out)
}

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
    pub fn next(&mut self, link: &mut wire::Link) -> rt::Result<char> {
        let b = self.byte(link)?;
        match b {
            // Le courtier reconnaît déjà CRLF, y compris au passage du dialogue OUI.
            13 => Ok('\n'),
            127 => Ok('\x08'),
            0..=127 => Ok(b as char),
            _ => {
                let len = match b {
                    0xc2..=0xdf => 2,
                    0xe0..=0xef => 3,
                    0xf0..=0xf4 => 4,
                    _ => return Ok('\0'),
                };
                let mut buf = [b; 4];
                for slot in &mut buf[1..len] {
                    let next = self.byte(link)?;
                    if next & 0xc0 != 0x80 {
                        self.held = Some(next);
                        return Ok('\0');
                    }
                    *slot = next;
                }
                Ok(core::str::from_utf8(&buf[..len])
                    .ok()
                    .and_then(|s| s.chars().next())
                    .unwrap_or('\0'))
            }
        }
    }
}

#[derive(Clone, Copy)]
struct BeforeEsc {
    mode: Mode,
    at: usize,
}
pub fn edit(e: &mut Editor, link: &mut wire::Link) -> rt::Result<()> {
    let mut keys = Keys::new();
    let mut escaped = None;
    let mut ss3: Option<BeforeEsc> = None;
    let mut sequence: Option<(BeforeEsc, bool)> = None;
    draw(e)?;
    loop {
        let key = keys.next(link)?;
        if let Some(before) = ss3.take() {
            if matches!(key, 'A' | 'B' | 'C' | 'D') {
                e.mode = before.mode;
                e.buf.at = before.at;
                e.arrow(key);
                draw(e)?;
                continue;
            }
            // Esc suivi de la commande normale O : le prochain caractère lève l'ambiguïté SS3.
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
                    e.arrow(key);
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
        } else if e.key(key, link) {
            return Ok(());
        }
        draw(e)?;
    }
}
