use alloc::string::String;
use nova_cli::{fileapi::Client, protocol as w, runtime as rt};
pub fn cd(fs: &mut Client, args: &[String]) -> rt::Result<()> {
    let [path] = args else {
        return Err(rt::BAD_ARGUMENT);
    };
    let dir = fs.directory(0, path.as_bytes(), w::DIR_ALL)?;
    let changed = fs.simple(w::CHDIR, dir);
    let closed = fs.simple(w::DIR_CLOSE, dir);
    changed.and(closed).map(|_| ())
}
