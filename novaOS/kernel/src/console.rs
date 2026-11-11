use crate::hardware::framebuffer::Screen;
use font8x8::legacy::{BASIC_LEGACY, LATIN_LEGACY};

const PROMPT: &str = "nova> ";
const CELL: usize = 16;
const ROW: usize = 20;

fn glyph(ch: char) -> [u8; 8] {
    match ch {
        ' '..='~' => BASIC_LEGACY[ch as usize],
        '\u{a0}'..='ÿ' => LATIN_LEGACY[ch as usize - 0xa0],
        '€' => [0x3c, 0x06, 0x1f, 0x06, 0x1f, 0x06, 0x3c, 0],
        'Ÿ' => [0x36, 0, 0x33, 0x33, 0x1e, 0x0c, 0x0c, 0],
        _ => BASIC_LEGACY[b'?' as usize],
    }
}

pub struct Console {
    screen: Screen,
    first_y: usize,
    y: usize,
    columns: usize,
    paper: u32,
    ink: u32,
}

impl Console {
    pub fn new(screen: Screen, banner_bottom: usize) -> Result<Self, &'static str> {
        let first_y = banner_bottom + CELL;
        let columns = screen.width.saturating_sub(2 * CELL) / CELL;
        if columns < PROMPT.len() + 2 || screen.height.saturating_sub(first_y) < ROW {
            return Err("Erreur : écran trop petit pour la saisie.");
        }
        let paper = screen.rgb(16, 24, 39);
        let ink = screen.rgb(232, 241, 251);
        Ok(Self {
            screen,
            first_y,
            y: first_y,
            columns,
            paper,
            ink,
        })
    }

    fn draw(&mut self, col: usize, ch: char) {
        let x = (col + 1) * CELL;
        for (row, bits) in glyph(ch).into_iter().enumerate() {
            for bit in 0..8 {
                if bits & (1 << bit) != 0 {
                    self.screen
                        .fill(x + 2 * bit, self.y + 2 * row, 2, 2, self.ink);
                }
            }
        }
    }

    pub fn line(&mut self, text: &[char], cursor: bool) {
        self.screen
            .fill(0, self.y, self.screen.width, ROW, self.paper);
        let visible = self.columns - PROMPT.len() - 1;
        let tail = &text[text.len().saturating_sub(visible)..];
        for (col, ch) in PROMPT.chars().chain(tail.iter().copied()).enumerate() {
            self.draw(col, ch);
        }
        if cursor {
            let x = (PROMPT.len() + tail.len() + 1) * CELL;
            self.screen.fill(x, self.y + CELL, CELL - 2, 2, self.ink);
        }
    }

    pub fn newline(&mut self) {
        if self.screen.height - self.y >= 2 * ROW {
            self.y += ROW;
        } else {
            self.screen.scroll_up(self.first_y, self.y + ROW, ROW);
        }
    }
}
