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
        let op = operand(p, &args).inspect_err(|_| {
            if let Some(option) = nova_cli::options::invalid_option(p, &args) {
                let _ = console::printable(
                    tty::ERROR,
                    &alloc::format!("option ou valeur non prise en charge : {option}\n"),
                );
            }
        })?;
        if matches!(op, Operand::Noop) {
            return Ok(());
        }
        if let Operand::Info(help) = op {
            let text = if help {
                nova_cli::options::usage(p)
            } else {
                concat!("NovaOS ", env!("CARGO_PKG_VERSION"))
            };
            console::write(tty::OUTPUT, text)?;
            return console::write(tty::OUTPUT, "\n");
        }
        let prep = rt::config(3);
        if prep == nova_cli::protocol::NOT_FOUND
            && matches!(&op, Operand::Remove(o, Program::Rm) if o.has(b'f'))
        {
            return Ok(());
        }
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
