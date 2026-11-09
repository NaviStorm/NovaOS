use crate::console::Console;

/// Une ligne Unicode validée. Ce type reste indépendant du pilote et de l’affichage.
pub struct Line {
    chars: [char; 128],
    len: usize,
}

impl Line {
    pub fn as_chars(&self) -> &[char] {
        &self.chars[..self.len]
    }

    fn edit(&mut self, ch: char) {
        match ch {
            '\u{8}' => self.len = self.len.saturating_sub(1),
            ch if !ch.is_control() && self.len < self.chars.len() => {
                self.chars[self.len] = ch;
                self.len += 1;
            }
            _ => {}
        }
    }
}

pub fn read_line(next_char: &mut impl FnMut() -> char, console: &mut Console) -> Line {
    let mut line = Line {
        chars: [' '; 128],
        len: 0,
    };
    loop {
        console.line(line.as_chars(), true);
        let ch = next_char();
        if ch == '\n' {
            console.line(line.as_chars(), false);
            console.newline();
            return line;
        }
        line.edit(ch);
    }
}
