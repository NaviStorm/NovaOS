//! L'offset de source avance uniquement après validation puis insertion du fragment.
use super::{
    buffer::Buffer,
    editor::{Editor, Mode},
};
use alloc::{string::String, vec::Vec};
use nova_cli::{console, runtime as rt, vi_wire as wire};

pub struct Tail {
    next: usize,
    len: usize,
    first_eol: bool,
    view_start: usize,
    cached_end: Option<Buffer>,
    skipped_rows: usize,
}
fn read(link: &mut wire::Link, start: usize, size: usize, mut v: Vec<u8>) -> rt::Result<Vec<u8>> {
    v.clear();
    v.try_reserve_exact(size).map_err(|_| rt::NO_MEMORY)?;
    while v.len() < size {
        let r = link.ask(wire::READ, &((start + v.len()) as u64).to_le_bytes())?;
        let part = &r.payload[1..r.len as usize];
        if part.is_empty() {
            return Err(rt::BAD_ARGUMENT);
        }
        v.extend_from_slice(&part[..part.len().min(size - v.len())]);
    }
    Ok(v)
}
pub fn prefix(link: &mut wire::Link, len: usize) -> rt::Result<(String, Tail)> {
    let mut t = Tail {
        next: 0,
        len,
        first_eol: false,
        view_start: 0,
        cached_end: None,
        skipped_rows: 0,
    };
    let s = if len <= wire::EAGER {
        String::from_utf8(read(link, 0, len, Vec::new())?).map_err(|_| rt::BAD_ARGUMENT)?
    } else {
        t.page(link, 0, Vec::new())?
    };
    t.next = s.len();
    t.first_eol = s.contains('\n');
    link.suffix = t.pending().then_some(t.next);
    Ok((s, t))
}
impl Tail {
    pub fn pending(&self) -> bool {
        self.next != self.len
    }
    fn page(&self, link: &mut wire::Link, at: usize, reuse: Vec<u8>) -> rt::Result<String> {
        let n = (self.len - at).min(wire::PAGE);
        let mut v = read(link, at, n, reuse)?;
        let used = wire::text_prefix(&v, at + n == self.len)?.len();
        v.truncate(used);
        if v.is_empty() {
            return Err(rt::BAD_ARGUMENT);
        }
        String::from_utf8(v).map_err(|_| rt::BAD_ARGUMENT)
    }
    fn more(&mut self, e: &mut Editor, link: &mut wire::Link) -> Result<(), &'static str> {
        let s = self.page(link, self.next, Vec::new()).map_err(reason)?;
        Buffer::validate(&s)?;
        e.buf.extend_source(&s)?;
        if !self.first_eol
            && let Some(n) = s.find('\n')
        {
            e.buf.source_eol(s[..n].ends_with('\r'));
            self.first_eol = true;
        }
        self.cached_end = None;
        self.next += s.len();
        link.suffix = self.pending().then_some(self.next);
        Ok(())
    }
    // Ne retenir que les dernières fenêtres pour afficher G. Le préfixe édité
    // et son historique restent dans le même Buffer, prêt pour gg et :w.
    fn end_view(&mut self, e: &mut Editor, link: &mut wire::Link) -> Result<bool, &'static str> {
        let mut at = self.next;
        let mut kept = alloc::collections::VecDeque::new();
        kept.try_reserve(8).map_err(|_| "mémoire insuffisante")?;
        let mut bytes = 0;
        let mut rows = e
            .buf
            .parts(0..e.buf.len())
            .iter()
            .map(|s| s.bytes().filter(|b| *b == b'\n').count())
            .sum::<usize>();
        let prefix_rows = rows;
        if self.cached_end.is_some() {
            link.ask(wire::VERIFY, &[]).map_err(reason)?;
            let window = self.cached_end.take().unwrap();
            e.parked = Some(core::mem::replace(&mut e.buf, window));
            e.row_base = prefix_rows + self.skipped_rows;
            return Ok(true);
        }
        while at < self.len {
            let reuse = if kept.len() == 8 {
                let old: String = kept.pop_front().unwrap();
                bytes -= old.len();
                old.into_bytes()
            } else {
                Vec::new()
            };
            let s = self.page(link, at, reuse).map_err(reason)?;
            Buffer::validate(&s)?;
            rows += s.bytes().filter(|b| *b == b'\n').count();
            at += s.len();
            bytes += s.len();
            kept.push_back(s);
        }
        let mut tail = String::new();
        tail.try_reserve_exact(bytes)
            .map_err(|_| "mémoire insuffisante")?;
        for s in kept {
            tail.push_str(&s);
        }
        let Some(first) = tail.find('\n') else {
            return Ok(false);
        };
        // Laisser assez de lignes de contexte même sur une grande surface.
        if tail[first + 1..].bytes().filter(|b| *b == b'\n').count() < 24 {
            return Ok(false);
        }
        tail.drain(..=first);
        self.view_start = self.len - tail.len();
        let offset = rows - tail.bytes().filter(|b| *b == b'\n').count();
        self.skipped_rows = offset - prefix_rows;
        let window = Buffer::new(tail)?;
        e.parked = Some(core::mem::replace(&mut e.buf, window));
        e.row_base = offset;
        Ok(true)
    }
    fn restore(&mut self, e: &mut Editor) -> usize {
        let at = e.buf.at;
        self.cached_end = Some(core::mem::replace(&mut e.buf, e.parked.take().unwrap()));
        e.row_base = 0;
        e.buf.len() + (self.view_start - self.next) + at
    }
    /// Aucune opération ne doit prendre la limite du préfixe pour la fin du fichier.
    pub fn before(
        &mut self,
        e: &mut Editor,
        key: char,
        arrow: bool,
        link: &mut wire::Link,
    ) -> Result<(), &'static str> {
        if !self.pending() {
            return Ok(());
        }
        if e.parked.is_some() {
            if e.mode == Mode::Normal && !arrow && key == 'g' && e.prefix_key() == Some('g') {
                self.restore(e);
                return Ok(());
            }
            if e.mode == Mode::Normal && !arrow && matches!(key, 'u' | '\x12') {
                self.restore(e);
                return Ok(());
            }
            let within = match e.mode {
                Mode::Command => true, // La publication prend le préfixe et son suffixe sur disque.
                Mode::Search => key != '\n',
                Mode::Insert => false,
                Mode::Normal if arrow => key != 'A' || e.buf.start() > 0,
                Mode::Normal => {
                    matches!(key, 'h' | 'j' | 'l' | '0' | '$' | 'G' | 'g' | ':' | '/')
                        || key == 'k' && e.buf.start() > 0
                }
            };
            if within {
                return Ok(());
            }
            let at = self.restore(e);
            while self.pending() {
                self.more(e, link)?;
            }
            e.buf.at = at;
            return Ok(());
        }
        if !arrow
            && e.mode == Mode::Normal
            && key == 'G'
            && e.prefix_key().is_none()
            && self.len - self.next > 8 * wire::PAGE
            && self.end_view(e, link)?
        {
            return Ok(());
        }
        let global = !arrow
            && match e.mode {
                Mode::Normal => matches!(key, 'G' | 'n'),
                Mode::Search => key == '\n',
                Mode::Command => false,
                Mode::Insert => false,
            };
        if global {
            while self.pending() {
                self.more(e, link)?;
            }
            return Ok(());
        }
        let need_eol = !arrow
            && ((!self.first_eol && e.mode == Mode::Insert && key == '\n')
                || (!self.first_eol && e.mode == Mode::Normal && matches!(key, 'o' | 'O')));
        let horizontal = arrow && key == 'C'
            || !arrow && e.mode == Mode::Normal && matches!(key, 'l' | 'a' | 'x');
        let vertical = arrow && key == 'B' || !arrow && e.mode == Mode::Normal && key == 'j';
        let linewise = !arrow && e.mode == Mode::Normal && matches!(key, '$' | 'o' | 'd');
        while self.pending() {
            let newline = e.buf.newline(e.buf.at);
            let need = need_eol && !self.first_eol
                || horizontal && e.buf.len() - e.buf.at <= 4
                || linewise && newline.is_none()
                || vertical && newline.and_then(|n| e.buf.newline(n + 1)).is_none();
            if !need {
                break;
            }
            self.more(e, link)?;
        }
        Ok(())
    }
}
pub fn reason(error: u64) -> &'static str {
    if error == wire::CONFLICT {
        "conflit : la source a changé"
    } else {
        console::reason(error)
    }
}
