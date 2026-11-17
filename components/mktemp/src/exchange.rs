use super::private::Private;
use alloc::vec::Vec;
use nova_cli::{protocol as fs, runtime as rt};
pub fn read(view: &Private, h: u64) -> Result<Vec<u8>, u64> {
    let stat = view.send(fs::frame(fs::STAT, 1, h))?;
    let len = usize::try_from(fs::word(&stat, fs::PAYLOAD + 16)).map_err(|_| rt::LIMIT)?;
    if !(8..=3072).contains(&len) {
        return Err(rt::BAD_ARGUMENT);
    }
    let mut bytes = Vec::with_capacity(len);
    while bytes.len() < len {
        let mut q = fs::frame(fs::READ, 1, h);
        let count = (len - bytes.len()).min(fs::FRAGMENT);
        fs::size(&mut q, count);
        fs::set(&mut q, 24, bytes.len() as u64);
        let a = view.send(q)?;
        if fs::length(&a) != count {
            return Err(rt::BAD_ARGUMENT);
        }
        bytes.extend_from_slice(&a[fs::PAYLOAD..fs::PAYLOAD + count]);
    }
    Ok(bytes)
}
pub fn write(view: &Private, h: u64, bytes: &[u8]) -> Result<(), u64> {
    for (i, chunk) in bytes.chunks(fs::FRAGMENT).enumerate() {
        let mut q = fs::frame(fs::WRITE, 1, h);
        q[2] = if i == 0 { fs::START } else { fs::CONTINUE };
        fs::set(&mut q, 24, (i * fs::FRAGMENT) as u64);
        fs::set(&mut q, 32, bytes.len() as u64);
        fs::size(&mut q, chunk.len());
        q[fs::PAYLOAD..fs::PAYLOAD + chunk.len()].copy_from_slice(chunk);
        view.send(q)?;
    }
    view.send(fs::frame(fs::SYNC, 1, h)).map(|_| ())
}
