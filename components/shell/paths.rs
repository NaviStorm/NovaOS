//! Les remontées utilisent la racine déjà détenue par la session de nsh.
//! Aucun chemin contenant « .. » ne traverse le protocole de fichiers.
use alloc::{string::String, vec::Vec};
use nova_cli::{fileapi::Client, protocol as w, runtime as rt};

fn directory_id(fs: &mut Client, path: &str) -> rt::Result<u64> {
    let st = fs.at(w::LOOKUP_AT, 0, path.as_bytes(), 0)?;
    if st[w::PAYLOAD + 24] != 2 {
        return Err(w::NOT_DIR);
    }
    Ok(w::word(&st, w::PAYLOAD))
}

pub fn join(parent: &str, name: &str) -> String {
    match parent {
        "" | "." => String::from(name),
        "/" => alloc::format!("/{name}"),
        p => alloc::format!("{p}/{name}"),
    }
}

pub fn resolve(fs: &mut Client, path: &str) -> rt::Result<String> {
    if path.is_empty() || path.contains("//") || path.chars().any(char::is_control) {
        return Err(rt::BAD_ARGUMENT);
    }
    if path.len() > w::PATH_MAX {
        return Err(rt::LIMIT);
    }
    let parent_steps = path.split('/').any(|s| s == "..");
    let mut result = if path.starts_with('/') {
        String::from("/")
    } else if parent_steps {
        let cwd = nova_cli::directory::cwd(fs)?;
        let id = w::word(&fs.simple(w::STAT, 0)?, w::PAYLOAD);
        if directory_id(fs, &cwd)? != id {
            return Err(rt::REVOKED);
        }
        cwd
    } else {
        String::from(".")
    };
    for part in path.split('/').filter(|s| !s.is_empty()) {
        match part {
            "." => {
                directory_id(fs, &result)?;
            }
            ".." => {
                let child = directory_id(fs, &result)?;
                let (parent, leaf) = result.rsplit_once('/').ok_or(rt::DENIED)?;
                if leaf.is_empty() {
                    return Err(rt::DENIED);
                }
                let parent = if parent.is_empty() { "/" } else { parent };
                let h = fs.directory(0, parent.as_bytes(), w::TRAVERSE)?;
                let linked = fs.at(w::LOOKUP_AT, h, leaf.as_bytes(), 0);
                fs.simple(w::DIR_CLOSE, h)?;
                if w::word(&linked?, w::PAYLOAD) != child {
                    return Err(rt::REVOKED);
                }
                result = parent.into();
            }
            name if name.len() <= 255 => result = join(&result, name),
            _ => return Err(rt::LIMIT),
        }
    }
    if result.len() > w::PATH_MAX {
        return Err(rt::LIMIT);
    }
    if path.ends_with('/') {
        directory_id(fs, &result)?;
    }
    Ok(result)
}

pub fn read_text(fs: &mut Client, path: &str, max: usize) -> rt::Result<String> {
    read_checked(fs, path, max, Client::read, &mut [0; w::FRAGMENT])
}

pub fn read_editor_text(fs: &mut Client, path: &str) -> rt::Result<String> {
    read_checked(
        fs,
        path,
        nova_cli::vi_wire::EAGER,
        Client::read_grouped,
        &mut [0; w::READ_WINDOW],
    )
}

fn read_checked(
    fs: &mut Client,
    path: &str,
    max: usize,
    read: fn(&mut Client, u64, u64, &mut [u8]) -> rt::Result<usize>,
    check: &mut [u8],
) -> rt::Result<String> {
    let h = fs.open_at(0, path.as_bytes(), w::READABLE)?;
    let read = (|| {
        let before = fs.simple(w::STAT, h)?;
        let len = w::word(&before, w::PAYLOAD + 16);
        if len > max as u64 {
            return Err(rt::LIMIT);
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(len as usize)
            .map_err(|_| rt::NO_MEMORY)?;
        bytes.resize(len as usize, 0);
        let n = read(fs, h, 0, &mut bytes)?;
        let after = fs.simple(w::STAT, h)?;
        if n != bytes.len()
            || w::word(&before, w::PAYLOAD) != w::word(&after, w::PAYLOAD)
            || w::word(&after, w::PAYLOAD + 16) != len
        {
            return Err(rt::REVOKED);
        }
        // READ peut publier atime sur P9. Le service ne sert aucun autre
        // écrivain pendant cette lecture synchrone sur le handle épinglé.
        // En cas de révision nouvelle, comparer aussi le contenu jusqu'à EOF.
        if w::word(&before, w::PAYLOAD + 8) != w::word(&after, w::PAYLOAD + 8) {
            let stride = check.len();
            for (i, chunk) in bytes.chunks(check.len()).enumerate() {
                let count = read(fs, h, (i * stride) as u64, check)?;
                if count != chunk.len() || &check[..count] != chunk {
                    return Err(rt::REVOKED);
                }
            }
            if fs.read(h, len, &mut check[..1])? != 0 {
                return Err(rt::REVOKED);
            }
        }
        String::from_utf8(bytes).map_err(|_| rt::BAD_ARGUMENT)
    })();
    fs.close(h)?;
    read
}
