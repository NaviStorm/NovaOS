//! Projection d'un flux vers CONSOLE. Les puits de fichiers et les pipes
//! reçoivent directement les trames. Aucun octet reconstruit ici ne leur revient.
use alloc::string::String;
use core::fmt::Write;
use unicode_width::UnicodeWidthChar;

#[derive(Default)]
pub(super) struct Decoder {
    bytes: [u8; 4],
    used: usize,
}

#[derive(Default)]
pub(super) struct Display {
    // Les deux flux partagent la position du terminal, mais pas leur décodeur.
    column: usize,
}

impl Display {
    fn scalar(&mut self, ch: char, out: &mut String) {
        match ch {
            '\n' => {
                out.push('\n');
                self.column = 0;
            }
            '\t' => {
                let count = 8 - self.column;
                out.push_str(&"        "[..count]);
                self.column = 0;
            }
            ch if nova_cli::tty::printable(ch) => {
                out.push(ch);
                self.column = (self.column + ch.width().unwrap_or(0)) % 8;
            }
            ch => {
                let start = out.len();
                write!(out, "\\u{{{:x}}}", u32::from(ch)).unwrap();
                self.column = (self.column + out.len() - start) % 8;
            }
        }
    }
    fn invalid(&mut self, byte: u8, out: &mut String) {
        write!(out, "\\x{byte:02x}").unwrap();
        self.column = (self.column + 4) % 8;
    }
}

impl Decoder {
    // L'appelant fournit une seule trame IPC (au plus 128 octets).
    pub(super) fn frame(&mut self, frame: &[u8], display: &mut Display, out: &mut String) {
        for &byte in frame {
            self.bytes[self.used] = byte;
            self.used += 1;
            loop {
                match core::str::from_utf8(&self.bytes[..self.used]) {
                    Ok(text) => {
                        for ch in text.chars() {
                            display.scalar(ch, out);
                        }
                        self.used = 0;
                        break;
                    }
                    Err(error) if error.error_len().is_none() => break,
                    Err(error) => {
                        // Le préfixe en attente est toujours un seul scalaire incomplet.
                        let n = error.error_len().unwrap();
                        for &invalid in &self.bytes[..n] {
                            display.invalid(invalid, out);
                        }
                        self.bytes.copy_within(n..self.used, 0);
                        self.used -= n;
                    }
                }
            }
        }
    }
    pub(super) fn finish(&mut self, display: &mut Display, out: &mut String) {
        for &byte in &self.bytes[..self.used] {
            display.invalid(byte, out);
        }
        self.used = 0;
    }
}
