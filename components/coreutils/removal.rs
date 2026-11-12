use alloc::{string::String, vec, vec::Vec};
use nova_cli::{
    command::{Operand, Program},
    console,
    directory::{entries_bounded, within},
    fileapi::Client,
    protocol as w, runtime as rt, tty,
};

const RIGHTS: u8 = w::TRAVERSE | w::ENUMERATE | w::REMOVE;

struct Work {
    path: Vec<String>,
    dir: bool,
    id: u64,
    after: bool,
}

fn confirm(verb: &str, name: &str) -> rt::Result<bool> {
    console::printable(tty::ERROR, &alloc::format!("{verb} {name} ? [o/N] "))?;
    let mut first = None;
    loop {
        let b = rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))?;
        if matches!(b, 10 | 13) {
            return Ok(matches!(first, Some(79 | 111 | 89 | 121)));
        }
        if first.is_none() {
            first = Some(b);
        }
    }
}

pub fn command(c: &mut Client, op: Operand) -> rt::Result<()> {
    let Operand::Remove(o, p) = op else {
        return Err(rt::BAD_ARGUMENT);
    };
    let name = o.files[0];
    let done = |label: &str| {
        if o.has(b'v') {
            console::printable(tty::OUTPUT, &alloc::format!("retiré {label}\n"))
        } else {
            Ok(())
        }
    };
    if p == Program::Rmdir || !(o.has(b'R') || o.has(b'r')) {
        if o.has(b'i') && !confirm("Retirer", name)? {
            return Ok(());
        }
        let opcode = if p == Program::Rmdir {
            w::RMDIR
        } else {
            w::UNLINK_AT
        };
        return match c.simple(opcode, 0) {
            Ok(_) => done(name),
            Err(w::NOT_FOUND) if o.has(b'f') => Ok(()),
            Err(e) => Err(e),
        };
    }
    let root = c.simple(w::STAT, 1)?;
    let mut work = vec![Work {
        path: Vec::new(),
        dir: root[w::PAYLOAD + 24] == 2,
        id: w::word(&root, w::PAYLOAD),
        after: false,
    }];
    let mut removed = 0usize;
    let mut remaining = 1024;
    let mut kept = alloc::collections::BTreeSet::new();
    let result = (|| {
        while let Some(item) = work.pop() {
            let label = if item.path.is_empty() {
                String::from(name)
            } else {
                alloc::format!("{}/{}", name.trim_end_matches('/'), item.path.join("/"))
            };
            if item.dir && !item.after {
                if o.has(b'i') && !confirm("Descendre dans", &label)? {
                    kept.insert(item.path);
                    continue;
                }
                let children = within(c, 1, &item.path, RIGHTS, |c, h| {
                    if w::word(&c.simple(w::STAT, h)?, w::PAYLOAD) != item.id {
                        return Err(rt::REVOKED);
                    }
                    entries_bounded(c, h, remaining)
                })?;
                remaining -= children.len();
                if item.path.len() >= 32 && !children.is_empty() {
                    return Err(rt::LIMIT);
                }
                let parent = item.path.clone();
                work.push(Work {
                    after: true,
                    ..item
                });
                for child in children.into_iter().rev() {
                    let mut path = parent.clone();
                    path.push(child.name);
                    let dir = match child.stat[w::PAYLOAD + 24] {
                        1 => false,
                        2 => true,
                        _ => return Err(w::UNSUPPORTED),
                    };
                    work.push(Work {
                        path,
                        dir,
                        id: w::word(&child.stat, w::PAYLOAD),
                        after: false,
                    });
                }
                continue;
            }
            if item.dir && kept.iter().any(|path| path.starts_with(&item.path)) {
                continue;
            }
            if o.has(b'i') && !confirm("Retirer", &label)? {
                kept.insert(item.path);
                continue;
            }
            let opcode = if item.dir { w::RMDIR } else { w::UNLINK_AT };
            if let Some((leaf, parent)) = item.path.split_last() {
                within(c, 1, parent, RIGHTS, |c, h| {
                    let st = c.at(w::LOOKUP_AT, h, leaf.as_bytes(), 0)?;
                    if w::word(&st, w::PAYLOAD) != item.id {
                        return Err(rt::REVOKED);
                    }
                    c.at(opcode, h, leaf.as_bytes(), 0).map(|_| ())
                })?;
            } else {
                // Le service refuse de retirer un objet encore ouvert.
                if item.dir {
                    c.simple(w::DIR_CLOSE, 1)?;
                } else {
                    c.close(1)?;
                }
                c.simple(opcode, 0)?;
            }
            removed += 1;
            done(&label)?;
        }
        Ok(())
    })();
    if result.is_err() {
        console::write(
            tty::ERROR,
            &alloc::format!("retraits confirmés avant erreur : {removed}\n"),
        )?;
    }
    result
}
