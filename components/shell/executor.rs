use super::{builtins, capabilities::Grant, parser::Invocation};
use alloc::vec::Vec;
use nova_cli::{
    command::{Program, operand},
    console::write,
    fileapi::Client,
    process, runtime as rt, tty,
};
pub fn execute(fs: &mut Client, cmd: Invocation, test: bool) -> rt::Result<bool> {
    match cmd.name.as_str() {
        "exit" => {
            return if cmd.args.is_empty() {
                Ok(false)
            } else {
                Err(rt::BAD_ARGUMENT)
            };
        }
        "cd" => return builtins::cd(fs, &cmd.args).map(|_| true),
        _ => {}
    }
    let Some(p) = Program::named(&cmd.name, test) else {
        write(tty::ERROR, "commande inconnue\n")?;
        return Ok(true);
    };
    let args: Vec<_> = cmd.args.iter().map(|s| s.as_str()).collect();
    let mut grant = Grant::new();
    let selection = operand(p, &args).and_then(|op| grant.select(fs, &op));
    let status = selection.err().unwrap_or(0);
    let channels = if status == 0 { grant.channels } else { (0, 0) };
    let result = (|| {
        let key = process::spawn(p, &cmd.args, channels, status)?;
        if p == Program::Probe && args == ["revocation"] {
            rt::value(rt::call(rt::IPC_REVOKE, channels.0, 0, 0))?;
        }
        let (fault, code) = process::wait(key)?;
        if test {
            write(
                tty::OUTPUT,
                &alloc::format!(
                    "[statut {} {}]\n",
                    if fault { "faute" } else { "sortie" },
                    code
                ),
            )?;
        }
        if fault {
            write(tty::ERROR, "processus interrompu\n")?;
        } else if !test && code != 0 {
            write(tty::ERROR, &alloc::format!("statut {code}\n"))?;
        }
        Ok(true)
    })();
    let freed = grant.release(fs);
    result.and_then(|v| freed.map(|_| v))
}
