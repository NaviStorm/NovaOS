use alloc::{string::String, vec::Vec};
use nova_cli::{command::Operand, directory, fileapi::Client, protocol as w, runtime as rt};

fn contents(c: &mut Client, src: u64, dst: u64, len: u64) -> rt::Result<()> {
    let mut block = Vec::new();
    block.try_reserve_exact(8192).map_err(|_| rt::NO_MEMORY)?;
    block.resize(8192, 0);
    let mut off = 0;
    while off < len {
        let n = (len - off).min(block.len() as u64) as usize;
        if c.read(src, off, &mut block[..n])? != n {
            return Err(rt::REFUSED);
        }
        c.write(dst, off, &block[..n])?;
        c.sync(dst)?;
        off += n as u64;
    }
    if len == 0 {
        c.sync(dst)?;
    }
    Ok(())
}

fn tree(c: &mut Client, src: u64, dst: u64, path: &mut Vec<String>) -> rt::Result<()> {
    let reader = w::TRAVERSE | w::ENUMERATE | w::FILE_READ;
    let writer = w::TRAVERSE | w::ADD | w::FILE_WRITE;
    let dirs = directory::within(c, src, path, reader, |c, input| {
        directory::within(c, dst, path, writer, |c, output| {
            let mut dirs = Vec::new();
            for entry in directory::entries(c, input)? {
                let name = entry.name.as_bytes();
                if entry.stat[w::PAYLOAD + 24] == 2 {
                    c.at(w::MKDIR_AT, output, name, 0)?;
                    dirs.push(entry.name);
                } else {
                    let source = c.open_at(input, name, w::READABLE)?;
                    let target = c.open_at(output, name, w::CREATE | w::WRITABLE)?;
                    contents(c, source, target, w::word(&entry.stat, w::PAYLOAD + 16))?;
                    c.close(source)?;
                    c.close(target)?;
                }
            }
            Ok(dirs)
        })
    })?;
    for name in dirs {
        path.push(name);
        tree(c, src, dst, path)?;
        path.pop();
    }
    Ok(())
}
pub fn command(c: &mut Client, op: Operand) -> rt::Result<()> {
    let Operand::Batch(o, _) = op else {
        return Err(rt::BAD_ARGUMENT);
    };
    let src = c.simple(w::STAT, 1)?;
    if let Some(dst) = super::transfer::exists(c, 2)? {
        if w::word(&src, w::PAYLOAD) == w::word(&dst, w::PAYLOAD) {
            return Err(rt::BAD_ARGUMENT);
        }
        return super::transfer::replacement(&o, o.files[1]);
    }
    let dir = src[w::PAYLOAD + 24] == 2;
    let mut q = w::frame(w::CREATE_NEW, c.session, 2);
    q[2] = u8::from(dir);
    let dst = w::word(&c.request(q)?, 16);
    if dir {
        tree(c, 1, dst, &mut Vec::new())?
    } else {
        contents(c, 1, dst, w::word(&src, w::PAYLOAD + 16))?
    }
    c.simple(if dir { w::DIR_CLOSE } else { w::CLOSE }, dst)?;
    c.simple(w::PUBLISH_COPY, 2)?;
    super::transfer::verbose(&o)
}
