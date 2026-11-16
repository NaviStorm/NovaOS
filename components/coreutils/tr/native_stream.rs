use nova_cli::{runtime as rt, tr::Filter};

/// Le compte de sortie peut être nul après suppression de toute une trame.
pub fn read(
    filter: &mut Filter,
    buf: &mut [u8],
    read_frame: impl FnOnce(&mut [u8]) -> rt::Result<usize>,
) -> rt::Result<(usize, bool)> {
    let n = read_frame(buf)?;
    let frame = buf.get_mut(..n).ok_or(rt::BAD_ARGUMENT)?;
    Ok((filter.apply(frame), n == 0))
}

/// Le lecteur restitue son extrémité après EOF, erreur de lecture ou erreur de sortie.
pub fn finish(result: rt::Result<()>, close: impl FnOnce() -> rt::Result<u64>) -> rt::Result<()> {
    let closed = close();
    result.and(closed.map(|_| ()))
}
