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
            if let Some(page) = nova_cli::documentation::lookup(env!("CARGO_BIN_NAME")) {
                return console::document(page, &args, help);
            }
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
        if p == Program::Cat && rt::config(1) != 0 && prep == tty::PIPE_PROBE_WAIT {
            // Le shell possède déjà l'extrémité racine des réponses fichier.
            // Aucun appel fichier n'est en vol avant ce rendez-vous du banc.
            let msg = rt::receive_for(rt::config(0), 15_000)?;
            if (msg.kind, msg.tag, msg.len, msg.payload[0]) != (1, tty::PIPE_PROBE_WAIT, 1, b'+') {
                return Err(rt::BAD_ARGUMENT);
            }
        } else if prep != 0 {
            return Err(prep);
        }
        let mut c = Client::scoped()?;
        command(&mut c, op)
    })();
    if let Err(e) = r {
        let reason = if p == Program::Cat && rt::config(1) != 0 && e == rt::CLOSED {
            "cat : canal fermé"
        } else {
            console::reason(e)
        };
        let _ = console::write(tty::ERROR, &alloc::format!("{reason}\n"));
        rt::exit(e)
    }
    rt::exit(0)
}
