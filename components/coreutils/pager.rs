use alloc::string::String;
use nova_cli::{command::Operand, console, fileapi::Client, runtime as rt, tty};
use unicode_width::UnicodeWidthChar;

struct Page {
    rows: usize,
    columns: usize,
    left: usize,
    column: usize,
    pending: String,
}
impl Page {
    fn flush(&mut self) -> rt::Result<()> {
        console::write(tty::OUTPUT, &self.pending)?;
        self.pending.clear();
        Ok(())
    }
    fn next(&mut self) -> rt::Result<bool> {
        self.flush()?;
        console::write(tty::OUTPUT, "--More--")?;
        let keep = loop {
            match rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))? {
                113 => break false,
                32 => {
                    self.left = self.rows;
                    break true;
                }
                10 | 13 => {
                    self.left = 1;
                    break true;
                }
                _ => {}
            }
        };
        console::write(tty::OUTPUT, "\n")?;
        Ok(keep)
    }
    fn push(&mut self, c: char) -> rt::Result<bool> {
        let width = c.width().unwrap_or(0);
        if c != '\n' && width > 0 && self.column + width > self.columns {
            self.column = 0;
            self.left -= 1;
        }
        if self.left == 0 && !self.next()? {
            return Ok(false);
        }
        self.pending.push(c);
        if c == '\n' {
            self.column = 0;
            self.left -= 1
        } else {
            self.column += width
        }
        if self.pending.len() >= 512 {
            self.flush()?
        }
        Ok(true)
    }
}
pub fn command(c: &mut Client, op: Operand) -> rt::Result<()> {
    let Operand::Batch(o, _) = op else {
        return Err(rt::BAD_ARGUMENT);
    };
    let dimension = |s: &str| {
        s.parse::<usize>()
            .ok()
            .filter(|n| *n >= 2 && *n <= u16::MAX.into())
            .ok_or(rt::BAD_ARGUMENT)
    };
    let rows = dimension(option_env!("NOVA_MORE_LINES").unwrap_or("24"))?;
    let columns = dimension(option_env!("NOVA_MORE_COLUMNS").unwrap_or("80"))?;
    for (i, _) in o.files.iter().enumerate() {
        let h = i as u64 + 1;
        let end = super::text::size(c, h)?;
        let mut page = Page {
            rows,
            columns,
            left: rows,
            column: 0,
            pending: String::new(),
        };
        let mut quit = false;
        super::text::scan(c, h, 0, end, |part| {
            let escaped;
            let text = match part {
                Ok(s) => s,
                Err(b) => {
                    escaped = alloc::format!("\\x{b:02x}");
                    &escaped
                }
            };
            for scalar in text.chars() {
                if scalar == '\n' || console::safe(scalar) {
                    if !page.push(scalar)? {
                        quit = true;
                        return Ok(false);
                    }
                } else {
                    for ch in alloc::format!("\\u{{{:x}}}", scalar as u32).chars() {
                        if !page.push(ch)? {
                            quit = true;
                            return Ok(false);
                        }
                    }
                }
            }
            Ok(true)
        })?;
        page.flush()?;
        if quit {
            break;
        }
    }
    Ok(())
}
