//! Projection du tampon sur l’écran série. Les coordonnées du texte sont indépendantes du terminal.
use super::editor::{Editor, Mode};
use alloc::{string::String, vec::Vec};
use core::fmt::Write;
use nova_cli::{console, runtime as rt, tty, vi_wire as wire};
use unicode_width::UnicodeWidthChar;

fn width(c: char) -> usize {
    if c == '\t' {
        4
    } else if console::safe(c) {
        c.width().unwrap_or(1).max(1)
    } else {
        1
    }
}

/// Une tabulation coupée reste un espace. Un glyphe large coupé ne déborde jamais sur la ligne suivante.
fn project(out: &mut String, chars: impl Iterator<Item = char>, left: usize, columns: usize) {
    let mut cell = 0;
    let right = left + columns;
    for c in chars {
        if cell >= right {
            break;
        }
        let next = cell + width(c);
        if next > left {
            if cell < left || next > right || c == '\t' {
                for _ in cell.max(left)..next.min(right) {
                    out.push(' ');
                }
            } else if console::safe(c) && c.width() != Some(0) {
                out.push(c);
            } else {
                out.push('�');
            }
        }
        cell = next;
    }
}

fn locate(link: &mut wire::Link, row: usize, col: usize) -> rt::Result<()> {
    let [r0, r1] = (row as u16 + 1).to_le_bytes();
    let [c0, c1] = (col as u16 + 1).to_le_bytes();
    link.ask(wire::LOCATE, &[r0, r1, c0, c1]).map(|_| ())
}

#[derive(Default)]
pub struct Screen {
    rows: usize,
    cols: usize,
    top: usize,
    left: usize,
    shown: Vec<Option<String>>,
    line: String,
    label: String,
    invalid: bool,
    cursor: Option<(usize, usize)>,
}
impl Screen {
    pub fn geometry(&mut self, link: &mut wire::Link, refresh: bool) -> rt::Result<()> {
        let reply = link.ask(wire::VIEW, &[u8::from(refresh)])?;
        if reply.len != 6 {
            return Err(rt::BAD_ARGUMENT);
        }
        let d = &reply.payload;
        let rows = u16::from_le_bytes([d[1], d[2]]) as usize;
        let cols = u16::from_le_bytes([d[3], d[4]]) as usize;
        if !(1..=usize::from(tty::MAX_AXIS)).contains(&rows)
            || !(1..=usize::from(tty::MAX_AXIS)).contains(&cols)
        {
            return Err(rt::LIMIT);
        }
        if self.rows != rows || self.cols != cols || d[5] != 0 || refresh {
            // En cas de pression mémoire pendant un agrandissement, réutiliser la surface disponible.
            // Le modèle de texte et le journal ne dépendent pas du cache de présentation.
            let _ = self
                .shown
                .try_reserve(rows.saturating_sub(self.shown.len()));
            self.line.clear();
            let _ = self.line.try_reserve(cols * 4);
            let rows = rows.min(self.shown.capacity());
            let cols = cols.min(self.line.capacity() / 4);
            if rows == 0 || cols == 0 {
                return Err(rt::NO_MEMORY);
            }
            self.shown.resize_with(rows, || None);
            self.label.clear();
            self.label
                .try_reserve(nova_cli::protocol::PATH_MAX + super::editor::PROMPT_MAX + 128)
                .map_err(|_| rt::NO_MEMORY)?;
            self.rows = rows;
            self.cols = cols;
            self.cursor = None;
            self.invalid = true;
        }
        Ok(())
    }
    fn paint(&mut self, row: usize, link: &mut wire::Link) -> rt::Result<()> {
        let cached = &mut self.shown[row];
        let valid = !self.invalid && cached.is_some();
        let old = cached.as_deref().unwrap_or("");
        if !valid || old != self.line {
            let prefix = if !valid || row >= self.rows.saturating_sub(2) {
                0
            } else {
                old.chars()
                    .zip(self.line.chars())
                    .take_while(|(a, b)| a == b)
                    .map(|(c, _)| c.len_utf8())
                    .sum()
            };
            let x = self.line[..prefix].chars().map(width).sum();
            locate(link, row, x)?;
            console::write(tty::OUTPUT, &self.line[prefix..])?;
            let end = x + self.line[prefix..].chars().map(width).sum::<usize>();
            if end < self.cols && (!valid || old.chars().map(width).sum::<usize>() > end) {
                link.ask(wire::CLEAR_TAIL, &[])?;
            }
            let saved = cached.get_or_insert_with(String::new);
            saved.clear();
            if saved.try_reserve(self.line.len()).is_ok() {
                saved.push_str(&self.line);
            } else {
                *cached = None;
            }
            self.cursor = None;
        }
        self.line.clear();
        Ok(())
    }
    pub fn draw(&mut self, e: &Editor, link: &mut wire::Link) -> rt::Result<()> {
        let row = e.buf.row();
        let start = e.buf.start();
        let cell: usize = e
            .buf
            .parts(start..e.buf.at)
            .into_iter()
            .flat_map(str::chars)
            .map(width)
            .sum();
        let height = self.rows.saturating_sub(2).max(1);
        self.top = self.top.clamp(row.saturating_sub(height - 1), row);
        self.left = self.left.clamp(cell.saturating_sub(self.cols - 1), cell);
        let mut begin = Some(start);
        for _ in self.top..row {
            begin = begin.map(|at| e.buf.start_at(at - 1));
        }
        for y in 0..height {
            if let Some(at) = begin {
                let end = e.buf.end_from(at);
                project(
                    &mut self.line,
                    e.buf.parts(at..end).into_iter().flat_map(str::chars),
                    self.left,
                    self.cols,
                );
                begin = e.buf.newline(end).map(|n| n + 1);
            } else {
                self.line.push('~');
            }
            self.paint(y, link)?;
        }
        let mode = match e.mode {
            Mode::Insert => "INSERTION",
            Mode::Normal => "NORMAL",
            Mode::Command => "COMMANDE",
            Mode::Search => "RECHERCHE",
        };
        let state = if e.buf.dirty() {
            "modifié"
        } else if e.exists {
            "enregistré"
        } else {
            "nouveau"
        };
        self.label.clear();
        let _ = write!(
            self.label,
            "vi [{mode}] {} [{state}] {}:{}",
            e.name,
            row + 1,
            e.buf.col() + 1
        );
        if self.rows >= 3 {
            project(&mut self.line, self.label.chars(), 0, self.cols);
            self.paint(height, link)?;
        }
        let mut target = (row - self.top, cell - self.left);
        if matches!(e.mode, Mode::Command | Mode::Search) {
            self.label.clear();
            self.label
                .push(if e.mode == Mode::Search { '/' } else { ':' });
            self.label.push_str(&e.prompt);
            let end = self.label.chars().map(width).sum::<usize>();
            let left = end.saturating_sub(self.cols - 1);
            project(&mut self.line, self.label.chars(), left, self.cols);
            target = (self.rows - 1, end - left);
        } else if self.rows > 1 {
            project(&mut self.line, e.message.chars(), 0, self.cols);
        }
        if self.rows > 1 || matches!(e.mode, Mode::Command | Mode::Search) {
            self.paint(self.rows - 1, link)?;
        }
        if self.cursor != Some(target) {
            locate(link, target.0, target.1)?;
            self.cursor = Some(target);
        }
        self.invalid = false;
        Ok(())
    }
}
