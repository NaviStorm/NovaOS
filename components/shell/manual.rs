//! Vue du manuel sur le terminal natif du shell.
use super::screen_input::{Input, screen};
use alloc::{string::String, vec::Vec};
use nova_cli::{console, documentation, protocol, runtime as rt, tty};
use unicode_width::UnicodeWidthChar;

fn lines(text: &str, width: usize) -> Vec<&str> {
    let mut result = Vec::new();
    for line in text.lines() {
        let mut start = 0;
        let mut cols = 0;
        for (i, c) in line.char_indices() {
            let size = c.width().unwrap_or(0);
            if cols + size > width && i > start {
                result.push(&line[start..i]);
                start = i;
                cols = 0;
            }
            cols += size;
        }
        result.push(&line[start..]);
    }
    result
}

fn row(n: usize, line: &str) -> rt::Result<()> {
    screen(tty::POSITION, ((n as u64 + 1) << 16) | 1)?;
    screen(tty::ERASE_EOL, 0)?;
    console::write(tty::OUTPUT, line)
}

fn read_search(input: &mut Input, n: usize, width: usize) -> rt::Result<String> {
    let mut bytes = Vec::new();
    loop {
        let display = core::str::from_utf8(&bytes).unwrap_or("");
        let prompt = alloc::format!("/{display}");
        row(n, lines(&prompt, width).last().copied().unwrap_or("/"))?;
        match input.key()? {
            13 | 10 => return String::from_utf8(bytes).map_err(|_| rt::BAD_ARGUMENT),
            3 | 27 => return Ok(String::new()),
            8 | 127 => {
                bytes.pop();
                while core::str::from_utf8(&bytes).is_err() {
                    bytes.pop();
                }
            }
            b if b >= 32 && b != 127 && bytes.len() < 96 => bytes.push(b),
            _ => (),
        }
        if let Ok(value) = core::str::from_utf8(&bytes)
            && !value.chars().all(console::safe)
        {
            bytes.clear();
        }
    }
}

pub fn open(args: &[&str]) -> rt::Result<u64> {
    let Some((name, path)) = args.split_first() else {
        console::write(tty::ERROR, "usage : man commande [sous-commande...]\n")?;
        return Ok(rt::BAD_ARGUMENT);
    };
    let Some(text) = documentation::lookup(name).and_then(|p| p.render(path, true)) else {
        console::write(tty::ERROR, "man : page absente\n")?;
        return Ok(protocol::NOT_FOUND);
    };
    let mut input = Input::default();
    let mut top: usize = 0;
    let mut query = String::new();
    let mut message = "";
    loop {
        let (rows, cols) = input.geometry(false)?;
        let width = usize::from(cols.saturating_sub(1).max(1));
        let height = usize::from(rows.saturating_sub(1));
        let body = lines(&text, width);
        let end = body.len().saturating_sub(height.max(1));
        top = top.min(end);
        for n in 0..height {
            row(n, body.get(top + n).copied().unwrap_or(""))?;
        }
        let status = alloc::format!(
            "man {name} {}/{}  q quitter  / chercher  {message}",
            top + 1,
            body.len()
        );
        row(height, lines(&status, width)[0])?;
        let Some(mut key) = input.event()? else {
            continue;
        };
        if key == 27 {
            if matches!(input.key()?, b'[' | b'O') {
                key = match input.key()? {
                    b'A' => b'k',
                    b'B' => b'j',
                    b'H' => b'g',
                    b'F' => b'G',
                    _ => continue,
                };
            } else {
                continue;
            }
        }
        message = "";
        match key {
            b'q' | 3 => {
                input.leave()?;
                return Ok(0);
            }
            b'j' | 13 | 10 => top = top.saturating_add(1),
            b'k' => top = top.saturating_sub(1),
            b' ' => top = top.saturating_add(height),
            b'b' => top = top.saturating_sub(height),
            b'g' => top = 0,
            b'G' => top = end,
            b'/' | b'n' => {
                if key == b'/' {
                    query = read_search(&mut input, height, width)?;
                }
                if !query.is_empty() {
                    let matches = (1..=body.len()).map(|i| (top + i) % body.len());
                    if let Some(index) = matches
                        .into_iter()
                        .find(|&i| body[i].contains(query.as_str()))
                    {
                        top = index;
                        message = "trouvé";
                    } else {
                        message = "introuvable";
                    }
                }
            }
            _ => (),
        }
    }
}
