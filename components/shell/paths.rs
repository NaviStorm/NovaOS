//! Les remontées utilisent la racine déjà détenue par la session de nsh.
//! Aucun chemin contenant « .. » ne traverse le protocole de fichiers.
use alloc::string::String;
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
