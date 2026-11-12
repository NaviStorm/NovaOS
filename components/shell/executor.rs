use super::{capabilities::Grant, paths};
use alloc::{string::String, vec::Vec};
use nova_cli::{
    command::{Program, operand},
    console::write,
    fileapi::Client,
    options, process, protocol as w, runtime as rt, tty,
};

pub fn execute(fs: &mut Client, p: Program, args: &[String], test: bool) -> rt::Result<u64> {
    let strings: Vec<_> = args.iter().map(String::as_str).collect();
    if p == Program::Vi
        && let Ok(nova_cli::command::Operand::Edit(name)) = operand(p, &strings)
    {
        return super::vi::run(fs, name, args, test, super::vi::Trial::Normal);
    }
    if test
        && p == Program::Probe
        && let [probe @ ("vi-revoque" | "vi-concurrence")] = strings.as_slice()
    {
        let trial = if *probe == "vi-revoque" {
            super::vi::Trial::Revoke
        } else {
            super::vi::Trial::Concurrent
        };
        let name = "vi-essai.nsh";
        return super::vi::run(fs, name, &[String::from(name)], true, trial);
    }
    // Le service conserve les vues isolées par paire pour cp/mv et par nom pour rm.
    // Valider le répertoire commun et toute l'expansion avant le premier lancement.
    if let Ok(o) = options::parse(p, &strings)
        && !o.help
        && !o.version
    {
        let transfer = matches!(p, Program::Cp | Program::Mv);
        let removal = matches!(p, Program::Rm | Program::Rmdir);
        if (transfer && o.files.len() > 2) || (removal && o.files.len() > 1) {
            let mut files = o.files.as_slice();
            let dest = if transfer {
                let (last, sources) = files.split_last().unwrap();
                let path = paths::resolve(fs, last)?;
                let h = fs.directory(0, path.as_bytes(), w::TRAVERSE)?;
                fs.simple(w::DIR_CLOSE, h)?;
                files = sources;
                Some(*last)
            } else {
                None
            };
            let mut parameters = Vec::new();
            for &c in b"finvrR" {
                if o.has(c) {
                    parameters.push(alloc::format!("-{}", char::from(c)));
                }
            }
            parameters.push(String::from("--"));
            for (i, name) in files.iter().enumerate() {
                let mut pair = parameters.clone();
                pair.push(String::from(*name));
                if let Some(dst) = dest {
                    pair.push(String::from(dst));
                }
                let status = launch(fs, p, &pair, test)?;
                if status != 0 {
                    write(
                        tty::ERROR,
                        &alloc::format!("lot interrompu après {i} opérande(s) traité(s)\n"),
                    )?;
                    return Ok(status);
                }
            }
            return Ok(0);
        }
    }
    launch(fs, p, args, test)
}

fn launch(fs: &mut Client, p: Program, args: &[String], test: bool) -> rt::Result<u64> {
    let words: Vec<_> = args.iter().map(String::as_str).collect();
    let mut grant = Grant::new();
    let selection = operand(p, &words).and_then(|op| grant.select(fs, &op));
    let status = selection.err().unwrap_or(0);
    let channels = if status == 0 { grant.channels } else { (0, 0) };
    let result = (|| {
        let key = process::spawn(p, args, channels, status)?;
        if p == Program::Probe
            && matches!(
                words.as_slice(),
                ["revocation" | "arbre-revoque" | "vide-revoque"]
            )
        {
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
        Ok(if fault { 128 } else { code })
    })();
    let freed = grant.release(fs);
    result.and_then(|v| freed.map(|_| v))
}
