//! Décodage en continu. Au plus trois octets UTF-8 sont reportés à la lecture suivante.
use nova_cli::{console, fileapi::Client, runtime as rt, tty};
pub fn display(c: &mut Client, lines: Option<usize>) -> rt::Result<()> {
    let mut remaining = lines.unwrap_or(usize::MAX);
    let mut buf = [0u8; 707];
    let mut carry = 0;
    let mut offset = 0u64;
    while remaining != 0 {
        let n = c.read(1, offset, &mut buf[carry..carry + 704])?;
        offset = offset.checked_add(n as u64).ok_or(rt::LIMIT)?;
        let mut end = n + carry;
        if lines.is_some() {
            for (i, &b) in buf[..end].iter().enumerate() {
                if b == b'\n' {
                    remaining -= 1;
                    if remaining == 0 {
                        end = i + 1;
                        break;
                    }
                }
            }
        }
        let mut pos = 0;
        while pos < end {
            match core::str::from_utf8(&buf[pos..end]) {
                Ok(text) => {
                    console::printable(tty::OUTPUT, text)?;
                    pos = end;
                }
                Err(e) => {
                    let valid = e.valid_up_to();
                    console::printable(
                        tty::OUTPUT,
                        core::str::from_utf8(&buf[pos..pos + valid]).unwrap(),
                    )?;
                    pos += valid;
                    if e.error_len().is_none() && n != 0 {
                        break;
                    }
                    console::write(tty::OUTPUT, &alloc::format!("\\x{:02x}", buf[pos]))?;
                    pos += 1;
                }
            }
        }
        if n == 0 {
            break;
        }
        carry = end - pos;
        buf.copy_within(pos..end, 0);
    }
    Ok(())
}
