use nova_cli::{command::Operand, fileapi::Client, protocol as w, runtime as rt};
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
    c.simple(w::MOVE_SELECTED, 1)?;
    super::transfer::verbose(&o)
}
