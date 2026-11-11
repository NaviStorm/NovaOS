//! Point d'entrée commun aux ELF. Aucun binaire de commande ne résout de chemin ambiant.
use nova_cli::{
    command::{Operand, Program, operand},
    console,
    fileapi::Client,
    process, runtime as rt, tty,
};
pub fn run(p: Program, command: impl FnOnce(&mut Client, Operand<'_>) -> rt::Result<()>) -> ! {
    let r = (|| {
        let args = process::arguments()?;
        let op = operand(p, &args)?;
        let prep = rt::config(3);
        if prep != 0 {
            return Err(prep);
        }
        let mut c = Client::scoped()?;
        command(&mut c, op)
    })();
    if let Err(e) = r {
        let _ = console::write(tty::ERROR, &alloc::format!("{}\n", console::reason(e)));
        rt::exit(e)
    }
    rt::exit(0)
}
