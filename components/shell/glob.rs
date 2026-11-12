//! Expansion de noms dans la session de nsh. Un composant ** possède une
//! transition sans descente et une transition vers chaque sous-répertoire.
use super::{lexer::Word, paths};
use alloc::{collections::BTreeSet, string::String, vec, vec::Vec};
use nova_cli::{directory::entries_bounded, fileapi::Client, protocol as w, runtime as rt, tty};

const DIRECTORY_LIMIT: usize = 128;
const ENTRY_LIMIT: usize = 1024;
const DEPTH_LIMIT: usize = 32;

enum Token {
    Literal(char),
    Star,
    One,
    Class(bool, Vec<(char, char)>),
}
struct Component {
    tokens: Vec<Token>,
    recursive: bool,
    magic: bool,
}

impl Component {
    fn parse(s: &str, offset: usize, word: &Word) -> rt::Result<Self> {
        let chars: Vec<_> = s
            .char_indices()
            .map(|(i, c)| (c, word.active.binary_search(&(offset + i)).is_ok()))
            .collect();
        let recursive = chars == [('*', true), ('*', true)];
        let mut tokens = Vec::new();
        let mut i = 0;
        let mut magic = false;
        while i < chars.len() {
            let token = match chars[i] {
                ('*', true) => Token::Star,
                ('?', true) => Token::One,
                ('[', true) => {
                    let neg = chars.get(i + 1) == Some(&('!', true));
                    let begin = i + 1 + usize::from(neg);
                    let end = (begin + 1..chars.len()).find(|&j| chars[j] == (']', true));
                    if let Some(end) = end {
                        let mut ranges = Vec::new();
                        let mut j = begin;
                        while j < end {
                            let lo = chars[j].0;
                            if j + 2 < end && chars[j + 1] == ('-', true) {
                                let hi = chars[j + 2].0;
                                if lo > hi {
                                    return Err(rt::BAD_ARGUMENT);
                                }
                                ranges.push((lo, hi));
                                j += 3;
                            } else {
                                ranges.push((lo, lo));
                                j += 1;
                            }
                        }
                        i = end;
                        Token::Class(neg, ranges)
                    } else {
                        Token::Literal('[')
                    }
                }
                (c, _) => Token::Literal(c),
            };
            magic |= !matches!(token, Token::Literal(_));
            tokens.push(token);
            i += 1;
        }
        Ok(Self {
            tokens,
            recursive,
            magic,
        })
    }

    fn matches(&self, name: &str) -> bool {
        if name.starts_with('.') && !matches!(self.tokens.first(), Some(Token::Literal('.'))) {
            return false;
        }
        let chars: Vec<_> = name.chars().collect();
        let mut reachable = vec![false; chars.len() + 1];
        reachable[0] = true;
        for token in &self.tokens {
            if matches!(token, Token::Star) {
                for j in 1..reachable.len() {
                    reachable[j] |= reachable[j - 1];
                }
            } else {
                for j in (1..reachable.len()).rev() {
                    let c = chars[j - 1];
                    let accepts = match token {
                        Token::Literal(t) => c == *t,
                        Token::One => true,
                        Token::Class(neg, ranges) => {
                            ranges.iter().any(|&(a, b)| a <= c && c <= b) != *neg
                        }
                        Token::Star => unreachable!(),
                    };
                    reachable[j] = reachable[j - 1] && accepts;
                }
                reachable[0] = false;
            }
        }
        reachable[chars.len()]
    }
}

struct Visit {
    path: String,
    states: Vec<bool>,
    ancestors: Vec<u64>,
}

fn zero_levels(pattern: &[Component], states: &mut [bool]) {
    for (i, part) in pattern.iter().enumerate() {
        if states[i] && part.recursive {
            states[i + 1] = true;
        }
    }
}

struct Budget {
    dirs: usize,
    entries: usize,
}

fn expand(
    fs: &mut Client,
    word: &Word,
    budget: &mut Budget,
    room: usize,
) -> rt::Result<Vec<String>> {
    let s = &word.text;
    if !word
        .active
        .iter()
        .any(|&i| matches!(s.as_bytes()[i], b'*' | b'?' | b'['))
    {
        return Ok(vec![s.clone()]);
    }
    if s.len() > w::PATH_MAX || s.contains("//") {
        return Err(rt::BAD_ARGUMENT);
    }
    let mut pattern = Vec::new();
    let mut prefix = if s.starts_with('/') {
        String::from("/")
    } else {
        String::new()
    };
    let mut offset = 0;
    for part in s.split('/') {
        if !part.is_empty() {
            if part.len() > 255 {
                return Err(rt::LIMIT);
            }
            let token = Component::parse(part, offset, word)?;
            if pattern.is_empty() && !token.magic {
                prefix = paths::join(&prefix, part);
            } else {
                if part == ".." {
                    return Err(rt::DENIED);
                }
                if part == "." {
                    return Err(rt::BAD_ARGUMENT);
                }
                pattern.push(token);
            }
        }
        offset += part.len() + 1;
    }
    if pattern.is_empty() {
        return Ok(vec![s.clone()]);
    }
    if pattern.len() > DEPTH_LIMIT {
        return Err(rt::LIMIT);
    }
    let only_dirs = s.ends_with('/');
    let mut initial = vec![false; pattern.len() + 1];
    initial[0] = true;
    zero_levels(&pattern, &mut initial);
    let mut pending = vec![Visit {
        path: prefix,
        states: initial,
        ancestors: Vec::new(),
    }];
    let mut found = BTreeSet::new();
    while let Some(visit) = pending.pop() {
        if budget.dirs == 0 {
            return Err(rt::LIMIT);
        }
        budget.dirs -= 1;
        let path = paths::resolve(
            fs,
            if visit.path.is_empty() {
                "."
            } else {
                &visit.path
            },
        )?;
        let dir = match fs.directory(0, path.as_bytes(), w::ENUMERATE | w::TRAVERSE) {
            Err(w::NOT_FOUND | w::NOT_DIR) if visit.ancestors.is_empty() => break,
            r => r?,
        };
        let listing = (|| {
            let id = w::word(&fs.simple(w::STAT, dir)?, w::PAYLOAD);
            if visit.ancestors.contains(&id) {
                return Err(w::CORRUPT);
            }
            Ok((id, entries_bounded(fs, dir, budget.entries)?))
        })();
        fs.simple(w::DIR_CLOSE, dir)?;
        let (id, entries) = listing?;
        budget.entries -= entries.len();
        // Un préfixe explicite suivi uniquement de ** accepte le répertoire lui-même.
        // À la racine implicite du motif, la session ne fabrique jamais « . ».
        if visit.ancestors.is_empty() && !visit.path.is_empty() && visit.states[pattern.len()] {
            let mut arg = visit.path.clone();
            if only_dirs && !arg.ends_with('/') {
                arg.push('/');
            }
            found.insert(arg);
            if found.len() > room {
                return Err(rt::LIMIT);
            }
        }
        for entry in entries {
            let is_dir = match entry.stat[w::PAYLOAD + 24] {
                1 => false,
                2 => true,
                _ => return Err(w::UNSUPPORTED),
            };
            let mut next = vec![false; pattern.len() + 1];
            let mut terminal_star = false;
            for (i, part) in pattern.iter().enumerate().filter(|(i, _)| visit.states[*i]) {
                if part.recursive {
                    if !entry.name.starts_with('.') {
                        next[i] |= is_dir;
                        terminal_star |= i + 1 == pattern.len();
                    }
                } else if part.matches(&entry.name) {
                    next[i + 1] = true;
                }
            }
            let consumed = next[pattern.len()] || terminal_star;
            zero_levels(&pattern, &mut next);
            let matched = consumed || (is_dir && next[pattern.len()]);
            let descend = is_dir && next[..pattern.len()].contains(&true);
            if !matched && !descend {
                continue;
            }
            let name = paths::join(&visit.path, &entry.name);
            if name.len() > w::PATH_MAX {
                return Err(rt::LIMIT);
            }
            if matched && (!only_dirs || is_dir) {
                let mut arg = name.clone();
                if only_dirs {
                    arg.push('/');
                }
                found.insert(arg);
                if found.len() > room {
                    return Err(rt::LIMIT);
                }
            }
            if descend {
                if visit.ancestors.len() >= DEPTH_LIMIT || pending.len() >= budget.dirs {
                    return Err(rt::LIMIT);
                }
                let mut ancestors = visit.ancestors.clone();
                ancestors.push(id);
                pending.push(Visit {
                    path: name,
                    states: next,
                    ancestors,
                });
            }
        }
    }
    if found.is_empty() {
        Ok(vec![s.clone()])
    } else {
        Ok(found
            .into_iter()
            .map(|name| {
                if name.starts_with('-') {
                    alloc::format!("./{name}")
                } else {
                    name
                }
            })
            .collect())
    }
}

pub fn arguments(fs: &mut Client, words: &[Word]) -> rt::Result<Vec<String>> {
    let mut result = Vec::new();
    let mut bytes = 1;
    let mut budget = Budget {
        dirs: DIRECTORY_LIMIT,
        entries: ENTRY_LIMIT,
    };
    for word in words {
        for name in expand(fs, word, &mut budget, tty::ARG_COUNT - result.len())? {
            bytes += 2 + name.len();
            if result.len() == tty::ARG_COUNT || bytes > tty::ARG_MAX {
                return Err(rt::LIMIT);
            }
            result.push(name);
        }
    }
    Ok(result)
}
