use nova_cli::{command::Operand, console, fileapi::Client, options::Options, runtime as rt, tty};
use unicode_width::UnicodeWidthChar;

fn render(o: &Options, counts: &[u64; 5], name: &str) -> rt::Result<()> {
    let flags = b"lwmcL";
    let defaults = !flags.iter().any(|&f| o.has(f));
    let mut out = alloc::string::String::new();
    use core::fmt::Write;
    for (&flag, n) in flags.iter().zip(counts) {
        if o.has(flag) || defaults && b"lwc".contains(&flag) {
            if !out.is_empty() {
                out.push(' ')
            }
            write!(out, "{n}").unwrap();
        }
    }
    console::printable(tty::OUTPUT, &alloc::format!("{out} {name}\n"))
}
pub fn command(c: &mut Client, op: Operand) -> rt::Result<()> {
    let Operand::Batch(o, _) = op else {
        return Err(rt::BAD_ARGUMENT);
    };
    let mut total = [0u64; 5];
    for (i, path) in o.files.iter().enumerate() {
        let h = i as u64 + 1;
        let mut counts = [0; 5];
        let mut word = false;
        let mut col = 0;
        let size = super::text::size(c, h)?;
        super::text::scan(c, h, 0, size, |part| {
            match part {
                Err(_) => {
                    counts[3] += 1;
                }
                Ok(text) => {
                    counts[3] += text.len() as u64;
                    for ch in text.chars() {
                        counts[2] += 1;
                        let space = ch.is_whitespace() || ch == '\u{2060}';
                        if !word && !space {
                            counts[1] += 1
                        }
                        word = !space;
                        match ch {
                            '\n' | '\r' | '\u{c}' => {
                                counts[4] = counts[4].max(col);
                                col = 0;
                                if ch == '\n' {
                                    counts[0] += 1
                                }
                            }
                            '\t' => col += 8 - col % 8,
                            _ => col += ch.width().unwrap_or(0) as u64,
                        }
                    }
                }
            }
            Ok(true)
        })?;
        counts[4] = counts[4].max(col);
        for j in 0..4 {
            total[j] = total[j].checked_add(counts[j]).ok_or(rt::LIMIT)?
        }
        total[4] = total[4].max(counts[4]);
        render(&o, &counts, path)?;
    }
    if o.files.len() > 1 {
        render(&o, &total, "total")?
    }
    Ok(())
}
