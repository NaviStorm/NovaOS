use super::input::{Input, Key};
use alloc::string::String;
use nova_cli::{
    console::{edit, write},
    runtime as rt, tty,
};
const PROMPT: &str = "nsh> ";
// Fenêtre en octets : le rendu d'un scalaire imprimable occupe au plus sa taille UTF-8.
const WINDOW: usize = 56;
fn start_at(s: &str, mut i: usize) -> usize {
    while !s.is_char_boundary(i) {
        i += 1
    }
    i
}
pub fn line(input: &mut Input, history: &super::history::History) -> rt::Result<String> {
    write(tty::OUTPUT, PROMPT)?;
    let mut s = String::new();
    let (mut pos, mut left) = (0, 0);
    let mut invalid = None;
    let mut index = history.len();
    let mut draft = (String::new(), 0);
    loop {
        match input.next()? {
            Key::Enter => {
                write(tty::OUTPUT, "\n")?;
                return invalid.map_or(Ok(s), Err);
            }
            Key::Invalid => {
                invalid = Some(rt::BAD_ARGUMENT);
                continue;
            }
            Key::Ignore => continue,
            Key::Char(c) => {
                if s.len() + c.len_utf8() > tty::LINE_MAX {
                    invalid = Some(rt::LIMIT);
                    continue;
                }
                s.insert(pos, c);
                pos += c.len_utf8();
                if pos == s.len() && pos - left <= WINDOW {
                    write(tty::OUTPUT, &s[pos - c.len_utf8()..])?;
                    continue;
                }
            }
            Key::Home => pos = 0,
            Key::End => pos = s.len(),
            Key::Delete if pos < s.len() => {
                s.remove(pos);
            }
            Key::Up if index > 0 => {
                if index == history.len() {
                    draft = (s.clone(), pos);
                }
                index -= 1;
                s = history.get(index).unwrap().clone();
                pos = s.len();
                left = 0;
            }
            Key::Down if index < history.len() => {
                index += 1;
                if let Some(entry) = history.get(index) {
                    s = entry.clone();
                    pos = s.len();
                } else {
                    (s, pos) = draft.clone();
                }
                left = 0;
            }
            Key::Erase if pos > 0 => {
                let i = s[..pos].char_indices().next_back().unwrap().0;
                s.remove(i);
                pos = i;
            }
            Key::Left if pos > 0 => {
                pos = s[..pos].char_indices().next_back().unwrap().0;
            }
            Key::Right if pos < s.len() => {
                pos += s[pos..].chars().next().unwrap().len_utf8();
            }
            _ => continue,
        }
        if pos < left || pos - left > WINDOW {
            left = start_at(&s, pos.saturating_sub(WINDOW / 2));
        }
        let mut right = (left + WINDOW).min(s.len());
        while !s.is_char_boundary(right) {
            right -= 1
        }
        // Récrire la fenêtre entière évite de compter les colonnes des accents et des idéogrammes.
        edit(0)?;
        write(tty::OUTPUT, PROMPT)?;
        write(
            tty::OUTPUT,
            "                                                        ",
        )?;
        edit(0)?;
        write(tty::OUTPUT, PROMPT)?;
        write(tty::OUTPUT, &s[left..right])?;
        edit(0)?;
        write(tty::OUTPUT, PROMPT)?;
        write(tty::OUTPUT, &s[left..pos])?;
    }
}
