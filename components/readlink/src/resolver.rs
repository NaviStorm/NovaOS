use super::options::Mode;
use alloc::{collections::BTreeSet, vec::Vec};

pub trait Namespace {
    type Error;
    fn cwd(&self) -> Result<Vec<u8>, Self::Error>;
    fn link(&self, path: &[u8]) -> Result<Vec<u8>, Self::Error>;
    fn search_directory(&self, path: &[u8]) -> Result<(), Self::Error>;
    fn ordinary(error: &Self::Error) -> bool;
    fn missing(error: &Self::Error) -> bool;
    fn empty() -> Self::Error;
    fn cycle() -> Self::Error;
}
fn pop(path: &mut Vec<u8>) {
    path.truncate(path.iter().rposition(|&b| b == b'/').unwrap_or(0).max(1));
}

pub fn resolve<N: Namespace>(ns: &N, name: &[u8], mode: Mode) -> Result<Vec<u8>, N::Error> {
    if mode == Mode::Raw {
        return ns.link(name);
    }
    if name.is_empty() {
        return Err(N::empty());
    }
    let mut path = if name.starts_with(b"/") {
        b"/".to_vec()
    } else {
        ns.cwd()?
    };
    let mut pending = name.to_vec();
    let mut i = 0;
    let mut seen = BTreeSet::new();
    let mut active = Vec::<(usize, Vec<u8>)>::new();
    let mut links = 0;
    while i < pending.len() {
        while active
            .last()
            .is_some_and(|(suffix, _)| pending.len() - i <= *suffix)
        {
            seen.remove(&active.pop().unwrap().1);
        }
        let start = i;
        while i < pending.len() && pending[i] != b'/' {
            i += 1;
        }
        let end = i;
        while i < pending.len() && pending[i] == b'/' {
            i += 1;
        }
        match &pending[start..end] {
            b"" | b"." => continue,
            b".." => {
                pop(&mut path);
                continue;
            }
            part => {
                if path.len() > 1 {
                    path.push(b'/');
                }
                path.extend_from_slice(part);
            }
        }
        match ns.link(&path) {
            Ok(target) => {
                links += 1;
                if links > 20 && !seen.insert(path.clone()) {
                    if mode == Mode::Missing {
                        continue;
                    }
                    return Err(N::cycle());
                }
                if links > 20 {
                    active.push((pending.len() - end, path.clone()));
                }
                if target.starts_with(b"/") {
                    path.truncate(1);
                } else {
                    pop(&mut path);
                }
                pending = [target.as_slice(), &pending[end..]].concat();
                i = 0;
            }
            Err(err) => {
                if mode == Mode::Missing {
                    continue;
                }
                if mode == Mode::Last && i == pending.len() && N::missing(&err) {
                    continue;
                }
                if !N::ordinary(&err) {
                    return Err(err);
                }
                if end != i {
                    ns.search_directory(&path)?;
                }
            }
        }
    }
    Ok(path)
}
