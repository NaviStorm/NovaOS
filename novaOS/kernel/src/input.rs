use crate::console::Console;
use alloc::vec::Vec;

const MAX_CHARS: usize = 128;

/// Une ligne Unicode validée. Ce type reste indépendant du pilote et de l’affichage.
pub struct Line {
    chars: Vec<char>,
}

impl Line {
    pub fn as_chars(&self) -> &[char] {
        &self.chars
    }

    fn edit(&mut self, ch: char) {
        match ch {
            '\u{8}' => {
                self.chars.pop();
            }
            ch if !ch.is_control() && self.chars.len() < MAX_CHARS => self.chars.push(ch),
            _ => {}
        }
    }
}

pub fn read_line(
    next_char: &mut impl FnMut() -> char,
    console: &mut Console,
) -> Result<Line, &'static str> {
    let mut line = Line { chars: Vec::new() };
    line.chars
        .try_reserve_exact(MAX_CHARS)
        .map_err(|_| "Mémoire insuffisante pour la ligne de saisie.")?;
    loop {
        console.line(line.as_chars(), true);
        let ch = next_char();
        if ch == '\n' {
            console.line(line.as_chars(), false);
            console.newline();
            return Ok(line);
        }
        line.edit(ch);
    }
}
