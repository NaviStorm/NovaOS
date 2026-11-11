use crate::{fileapi::Client, protocol as w, runtime as rt};
use alloc::{string::String, vec::Vec};
pub struct Entry {
    pub name: String,
    pub stat: w::Frame,
}
pub fn entries(c: &mut Client, h: u64) -> rt::Result<Vec<Entry>> {
    let mut rows = Vec::new();
    let mut after = Vec::new();
    let mut generation = 0;
    loop {
        let mut name = Vec::new();
        loop {
            let a = c.readdir_fragment(h, &after, name.len() as u64, generation)?;
            generation = w::word(&a, 24);
            let n = w::length(&a);
            let len = w::word(&a, 32);
            if len == 0 && name.is_empty() {
                return Ok(rows);
            }
            if n == 0 || n > w::FRAGMENT || len > 255 || name.len() as u64 + n as u64 > len {
                return Err(rt::REFUSED);
            }
            name.extend_from_slice(&a[w::PAYLOAD..w::PAYLOAD + n]);
            if name.len() as u64 == len {
                break;
            }
        }
        let stat = c.at(w::LOOKUP_AT, h, &name, 0)?;
        after = name.clone();
        rows.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
        rows.push(Entry {
            name: String::from_utf8(name).map_err(|_| rt::REFUSED)?,
            stat,
        });
    }
}

/// Traverser depuis une capacité épinglée en ne gardant qu'un curseur dérivé.
/// La profondeur persistante ne consomme pas un handle ouvert par ancêtre.
pub fn within<T>(
    client: &mut Client,
    root: u64,
    path: &[String],
    rights: u8,
    f: impl FnOnce(&mut Client, u64) -> rt::Result<T>,
) -> rt::Result<T> {
    let mut dir = root;
    for name in path {
        let step = client.directory(dir, name.as_bytes(), rights);
        if dir != root {
            client.simple(w::DIR_CLOSE, dir)?;
        }
        dir = step?;
    }
    let result = f(client, dir);
    if dir != root {
        client.simple(w::DIR_CLOSE, dir)?;
    }
    result
}
