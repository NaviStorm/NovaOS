//! Point d'entrée de recette réservé à l'ELF @probe du profil cli-test.
use super::*;
use nova_cli::{directory, fileapi::Client, process, protocol as w, runtime as rt};
fn trace(s: &str) {
    for chunk in s.as_bytes().chunks(rt::DEBUG_MAX) {
        rt::call(
            rt::DEBUG_WRITE,
            chunk.as_ptr() as u64,
            chunk.len() as u64,
            0,
        );
    }
}
fn read_handle(c: &mut Client, h: u64) -> rt::Result<Vec<u8>> {
    let st = c.simple(w::STAT, h)?;
    let len = w::word(&st, w::PAYLOAD + 16) as usize;
    if len > 1 << 20 {
        return Err(rt::LIMIT);
    }
    let mut out = vec![0; len];
    for (i, part) in out.chunks_mut(4096).enumerate() {
        if c.read(h, (i * 4096) as u64, part)? != part.len() {
            return Err(rt::REVOKED);
        }
    }
    let after = c.simple(w::STAT, h)?;
    if st[w::PAYLOAD..w::PAYLOAD + 24] != after[w::PAYLOAD..w::PAYLOAD + 24] {
        return Err(rt::REVOKED);
    }
    Ok(out)
}
fn number(data: &mut &[u8]) -> usize {
    let (v, rest) = data.split_at(4);
    *data = rest;
    u32::from_le_bytes(v.try_into().unwrap()) as usize
}
struct Memory<'a> {
    files: Vec<(&'a [u8], &'a [u8], u8)>,
    stdin: &'a [u8],
    out: [Vec<u8>; 2],
    opens: usize,
}
impl<'a> Memory<'a> {
    fn node(&self, path: &[u8], follow: bool) -> Option<(usize, &'a [u8], u8)> {
        let (i, (_, v, k)) = self.files.iter().enumerate().find(|(_, v)| v.0 == path)?;
        if *k == 2 && follow {
            return self.node(v, false);
        }
        Some((i, *v, *k))
    }
}
impl Platform for Memory<'_> {
    fn stat(&mut self, p: &[u8], o: &Options) -> Result<Option<Meta>, String> {
        self.opens += 1;
        if p == b"-" {
            return Ok(Some(Meta::file()));
        }
        Ok(self
            .node(p, !o.has("no-dereference"))
            .map(|(i, _, k)| Meta {
                kind: match k {
                    1 => Kind::Dir,
                    2 => Kind::Link,
                    _ => Kind::File,
                },
                id: Some((1, i as u64)),
                stamp: if o.style == "context" {
                    b"Tue Nov 14 22:13:20 2023".to_vec()
                } else {
                    b"2023-11-14 22:13:20.000000000 +0000".to_vec()
                },
            }))
    }
    fn read(&mut self, p: &[u8]) -> Result<Vec<u8>, String> {
        self.opens += 1;
        if p == b"-" {
            return Ok(core::mem::take(&mut self.stdin).to_vec());
        }
        match self.node(p, true) {
            Some((_, _, 3)) => Err(format!("{}: Permission denied", txt(p))),
            Some((_, v, _)) => Ok(v.to_vec()),
            _ => Err(format!("{}: No such file or directory", txt(p))),
        }
    }
    fn list(&mut self, p: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        let prefix = [p, b"/"].concat();
        Ok(self
            .files
            .iter()
            .filter_map(|(name, _, _)| {
                name.strip_prefix(prefix.as_slice())
                    .filter(|s| !s.contains(&b'/'))
                    .map(<[u8]>::to_vec)
            })
            .collect())
    }
    fn link(&mut self, p: &[u8]) -> Result<Vec<u8>, String> {
        Ok(self.node(p, false).unwrap().1.to_vec())
    }
    fn emit(&mut self, e: bool, b: &[u8]) -> Result<(), String> {
        self.out[e as usize].extend(b);
        Ok(())
    }
}
fn vectors(buf: &[u8]) -> u8 {
    let mut d = buf;
    let pool: Vec<_> = (0..number(&mut d))
        .map(|_| {
            let n = number(&mut d);
            let (v, rest) = d.split_at(n);
            d = rest;
            v
        })
        .collect();
    let files = (0..number(&mut d))
        .map(|_| {
            (
                pool[number(&mut d)],
                pool[number(&mut d)],
                number(&mut d) as u8,
            )
        })
        .collect();
    let mut mem = Memory {
        files,
        stdin: b"",
        out: [vec![], vec![]],
        opens: 0,
    };
    let count = number(&mut d);
    let mut fail = 0;
    for _ in 0..count {
        let id = number(&mut d);
        let args: Vec<_> = (0..number(&mut d)).map(|_| pool[number(&mut d)]).collect();
        mem.stdin = pool[number(&mut d)];
        let lang = pool[number(&mut d)];
        let rc = number(&mut d);
        let expected = [pool[number(&mut d)], pool[number(&mut d)]];
        let doc = number(&mut d) != 0;
        mem.out.iter_mut().for_each(Vec::clear);
        mem.opens = 0;
        let fr =
            locale::Language::from_tag(core::str::from_utf8(lang).ok()) == locale::Language::French;
        let actual = execute(&mut mem, &args, b"diff", fr, false, false);
        if actual as usize != rc
            || mem.out[0] != expected[0]
            || mem.out[1] != expected[1]
            || doc && mem.opens != 0
        {
            fail += 1;
            trace(&format!(
                "DIFF-ECART id={id} rc={actual}/{rc} out={} err={}",
                mem.out[0] == expected[0],
                mem.out[1] == expected[1]
            ))
        }
    }
    trace(&format!("DIFF-VECTEURS total={count} ecarts={fail}"));
    u8::from(fail != 0)
}
struct Selected {
    c: Client,
    tree: bool,
    out: [Vec<u8>; 2],
}
impl Selected {
    fn within<T>(
        &mut self,
        p: &[u8],
        op: impl FnOnce(&mut Client, u64, Option<&[u8]>) -> rt::Result<T>,
    ) -> Result<T, String> {
        let path = core::str::from_utf8(p).map_err(|_| "UTF-8")?;
        let mut parts = path.split('/');
        let root = match parts.next() {
            Some("a") => 1,
            Some("b") => 2,
            _ => return Err("chemin de recette inconnu".into()),
        };
        let mut dirs: Vec<String> = parts.map(String::from).collect();
        let leaf = dirs.pop();
        directory::within(&mut self.c, root, &dirs, w::DIR_RO, |c, h| {
            op(c, h, leaf.as_deref().map(str::as_bytes))
        })
        .map_err(|e| format!("{}: erreur P9 {e}", txt(p)))
    }
}
impl Platform for Selected {
    fn stat(&mut self, p: &[u8], _: &Options) -> Result<Option<Meta>, String> {
        self.within(p, |c, h, leaf| {
            let st = if let Some(s) = leaf {
                c.at(w::LOOKUP_AT, h, s, 0)?
            } else {
                c.simple(w::STAT, h)?
            };
            Ok(Some(Meta {
                kind: if st[w::PAYLOAD + 24] == 2 {
                    Kind::Dir
                } else {
                    Kind::File
                },
                id: Some((1, w::word(&st, w::PAYLOAD))),
                stamp: vec![],
            }))
        })
    }
    fn read(&mut self, p: &[u8]) -> Result<Vec<u8>, String> {
        self.within(p, |c, h, leaf| {
            if let Some(s) = leaf {
                let f = c.open_at(h, s, w::READABLE)?;
                let data = read_handle(c, f);
                c.close(f)?;
                data
            } else {
                read_handle(c, h)
            }
        })
    }
    fn list(&mut self, p: &[u8]) -> Result<Vec<Vec<u8>>, String> {
        self.within(p, |c, h, leaf| {
            let d = if let Some(s) = leaf {
                c.directory(h, s, w::DIR_RO)?
            } else {
                h
            };
            let mut v = vec![];
            let result = directory::visit_names(c, d, 256, |_, s| {
                v.push(s.as_bytes().to_vec());
                Ok(())
            });
            if leaf.is_some() {
                c.simple(w::DIR_CLOSE, d)?;
            }
            result?;
            Ok(v)
        })
    }
    fn emit(&mut self, err: bool, b: &[u8]) -> Result<(), String> {
        self.out[err as usize].extend(b);
        Ok(())
    }
}
fn run_case() -> rt::Result<u8> {
    let args = process::arguments()?;
    let [case] = args.as_slice() else {
        return Err(rt::BAD_ARGUMENT);
    };
    let mut c = Client::scoped()?;
    if *case == "diff-corpus" {
        let bytes = read_handle(&mut c, 1)?;
        return Ok(vectors(&bytes));
    }
    let mut p = Selected {
        c,
        tree: *case == "diff-tree",
        out: [vec![], vec![]],
    };
    let args: &[&[u8]] = if p.tree {
        &[b"-r", b"a", b"b"]
    } else {
        &[b"a", b"b"]
    };
    let rc = execute(&mut p, args, b"diff", false, false, false);
    for (side, v) in p.out.iter().enumerate() {
        for bytes in v.chunks(64) {
            use core::fmt::Write;
            let mut s = format!("DIFF-OCTETS {side} ");
            for b in bytes {
                let _ = write!(s, "{b:02x}");
            }
            trace(&s)
        }
    }
    trace(&format!(
        "DIFF-RESULTAT rc={rc} stdout={} stderr={}",
        p.out[0].len(),
        p.out[1].len()
    ));
    Ok(rc)
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let rc = run_case().unwrap_or_else(|e| {
        trace(&format!("DIFF-ERREUR {e}"));
        2
    });
    rt::exit(rc.into())
}
