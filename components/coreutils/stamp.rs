use nova_cli::{command::Operand, fileapi::Client, protocol as w, runtime as rt};
pub fn command(c: &mut Client, op: Operand) -> rt::Result<()> {
    let Operand::Batch(o, _) = op else {
        return Err(rt::BAD_ARGUMENT);
    };
    let mut which = u8::from(o.has(b'a')) | (u8::from(o.has(b'm')) << 1);
    if which == 0 {
        which = 3;
    }
    for i in 0..o.files.len() {
        let mut q = w::frame(w::TOUCH_SELECTED, c.session, i as u64 + 1);
        q[2] = which | if o.has(b'c') { 4 } else { 0 };
        c.request(q)?;
    }
    Ok(())
}
