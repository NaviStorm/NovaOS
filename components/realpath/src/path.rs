use crate::options::{Existence, Links, Settings};
use alloc::{collections::VecDeque, vec::Vec};

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    Missing,
    NotDirectory,
    Loop,
    Unavailable,
    System(i32),
}
pub trait Namespace {
    fn cwd(&mut self) -> Result<Vec<u8>, Error>;
    fn link(&mut self, path: &[u8]) -> Result<Option<Vec<u8>>, Error>;
    fn check(&mut self, path: &[u8], directory: bool) -> Result<(), Error>;
}
fn parts(path: &[u8]) -> impl Iterator<Item = &[u8]> {
    path.split(|&c| c == b'/').filter(|c| !c.is_empty())
}
fn parent(path: &mut Vec<u8>) {
    path.truncate(path.iter().rposition(|&c| c == b'/').unwrap_or(0).max(1))
}
fn pass(
    fs: &mut impl Namespace,
    name: &[u8],
    mode: Existence,
    follow: bool,
) -> Result<Vec<u8>, Error> {
    if name.is_empty() {
        return Err(Error::Missing);
    }
    let mut path = if name.starts_with(b"/") {
        b"/".to_vec()
    } else {
        fs.cwd()?
    };

    let mut slash = name.ends_with(b"/");
    // A link may reappear later in a valid operand. Only recursive expansion cycles.
    enum Marker {
        End(Vec<u8>),
        Name(Vec<u8>),
    }
    let mut queue: VecDeque<_> = parts(name).map(|s| Marker::Name(s.to_vec())).collect();
    let mut active = Vec::new();
    while let Some(item) = queue.pop_front() {
        let part = match item {
            Marker::Name(v) => v,
            Marker::End(v) => {
                debug_assert_eq!(active.last(), Some(&v));
                active.pop();
                continue;
            }
        };
        if part == b"." {
            continue;
        }
        if part == b".." {
            parent(&mut path);
            continue;
        }
        if path.len() > 1 {
            path.push(b'/')
        }
        path.extend(&part);
        let mut tail = queue.iter().filter_map(|m| match m {
            Marker::Name(v) => Some(v.as_slice()),
            _ => None,
        });
        let next = tail.next();
        let last = next.is_none();
        let dir = match next {
            None => slash,
            Some(b"..") => true,
            Some(b".") => tail.find(|p| *p != b".").is_none_or(|p| p == b".."),
            _ => false,
        };
        let result = if follow { fs.link(&path) } else { Ok(None) };
        let result = match result {
            Ok(Some(dest)) => {
                if active.contains(&path) {
                    if mode == Existence::Missing {
                        continue;
                    }
                    return Err(Error::Loop);
                }
                queue.push_front(Marker::End(path.clone()));
                active.push(path.clone());
                if last {
                    slash |= dest.ends_with(b"/")
                }
                for p in dest.rsplit(|&c| c == b'/').filter(|s| !s.is_empty()) {
                    queue.push_front(Marker::Name(p.to_vec()))
                }
                if dest.starts_with(b"/") {
                    path.truncate(1)
                } else {
                    parent(&mut path)
                }
                continue;
            }
            Ok(None) if mode != Existence::Missing && (dir || (!follow && last)) => {
                fs.check(&path, dir)
            }
            Ok(None) => Ok(()),
            Err(e) => Err(e),
        };
        if let Err(e) = result {
            if e == Error::Unavailable
                || !(mode == Existence::Missing
                    || (mode == Existence::Last && last && e == Error::Missing))
            {
                return Err(e);
            }
        }
    }
    Ok(path)
}
pub fn resolve(fs: &mut impl Namespace, name: &[u8], opt: &Settings) -> Result<Vec<u8>, Error> {
    let path = pass(fs, name, opt.exist, opt.links == Links::Physical)?;
    if opt.links == Links::Logical {
        pass(fs, &path, opt.exist, true)
    } else {
        Ok(path)
    }
}
fn below(path: &[u8], base: &[u8]) -> bool {
    base == b"/" || path == base || path.strip_prefix(base).is_some_and(|s| s.starts_with(b"/"))
}
pub struct Relative {
    origin: Option<Vec<u8>>,
    base: Option<Vec<u8>>,
}
impl Relative {
    pub fn new<'a>(fs: &mut impl Namespace, opt: &Settings<'a>) -> Result<Self, (&'a [u8], Error)> {
        let mut dir = |name: &'a [u8]| {
            let p = resolve(fs, name, opt).map_err(|e| (name, e))?;
            if opt.exist == Existence::Existing {
                fs.check(&p, true).map_err(|e| (name, e))?
            }
            Ok(p)
        };
        let origin = opt.to.or(opt.base).map(&mut dir).transpose()?;
        let base = if opt.to == opt.base || opt.to.is_none() {
            origin.clone()
        } else {
            opt.base.map(dir).transpose()?
        };
        Ok(Self { origin, base })
    }
    pub fn display(&self, path: Vec<u8>) -> Vec<u8> {
        let Some(from) = &self.origin else {
            return path;
        };
        if self
            .base
            .as_ref()
            .is_some_and(|b| !below(from, b) || !below(&path, b))
        {
            return path;
        }
        let a: Vec<_> = parts(from).collect();
        let b: Vec<_> = parts(&path).collect();
        let same = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        let mut names = alloc::vec![b"..".as_slice();a.len()-same];
        names.extend_from_slice(&b[same..]);
        if names.is_empty() {
            b".".to_vec()
        } else {
            names.join(&b'/')
        }
    }
}
