use super::{capabilities::Grant, paths};
use alloc::{string::String, vec::Vec};
use nova_cli::{
    command::{Program, operand},
    console::write,
    fileapi::Client,
    options, process, protocol as w, runtime as rt, tty,
};

/// Le seul état transmis est la langue du document. Les deux canaux métier sont nuls.
pub fn document(p: Program, help: bool, lang: nova_cli::documentation::Language) -> rt::Result<u64> {
    let argv = [
        String::from(if help { "--help" } else { "--version" }),
        String::from("--"),
        String::from(lang.select("nsh-doc=en", "nsh-doc=fr")),
    ];
    let key = process::spawn(p, &argv, (0, 0), 0)?;
    let (fault, status) = process::wait(key)?;
    Ok(if fault { 128 } else { status })
}
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
    let o = match options::parse(p, &strings) {
        Ok(options) => options,
        Err(code) => {
            let arg = options::invalid_option(p, &strings).unwrap_or("");
            write(tty::ERROR, &alloc::format!("argument invalide, option ou valeur non prise en charge : {arg}\n"))?;
            if test {
                write(tty::OUTPUT, &alloc::format!("[statut sortie {code}]\n"))?;
            }
            return Ok(code);
        }
    };
    if !o.help && !o.version {
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

#[derive(Copy, Clone, PartialEq)]
pub enum PipeTrial { Normal, Pressure, Early, Revoke, Admission }

pub fn pipeline(fs: &mut Client, args: &[String], test: bool, trial: PipeTrial) -> rt::Result<u64> {
    let words: Vec<_> = args.iter().map(String::as_str).collect();
    let op = operand(Program::Cat, &words)?;
    let nova_cli::command::Operand::Batch(_, Program::Cat) = &op else { return Err(rt::BAD_ARGUMENT); };
    let root = rt::value(rt::call(tty::SPAWN, tty::PIPE_OPEN, 0, 0))?;
    let mut grant = Grant::new();
    let prep = grant.select(fs, &op).err().unwrap_or(0);
    let channels = if prep == 0 { grant.channels } else { (0, 0) };
    let close = || rt::value(rt::call(rt::IPC_CLOSE, root, 0, 0));
    let mut closed_early = false;
    let run = (|| {
        let writer = process::pipe_spawn(Program::Cat, args, channels, prep, root)?;
        let reader_args = [String::from(if trial == PipeTrial::Early { "--invalide" } else { "-l" })];
        // Une clé étrangère refuse le second lancement. Retirer les extrémités
        // avant d'attendre le producteur, qui peut déjà dormir sur une file pleine.
        let reader = (|| {
            if trial != PipeTrial::Normal {
                // Le banc libère ce verrou de scénario après la trace du SEND bloqué.
                if rt::value(rt::call(tty::CONSOLE, tty::INPUT.into(), 0, 0))? != u64::from(b'+') {
                    return Err(rt::BAD_ARGUMENT);
                }
            }
            process::pipe_spawn(Program::Wc, &reader_args, (0, 0), 0,
                if trial == PipeTrial::Admission { 0 } else { root })
        })();
        let cancellation = if reader.is_err() || trial == PipeTrial::Revoke {
            let result = rt::value(rt::call(rt::IPC_REVOKE, root, 0, 0));
            if result.is_err() {
                closed_early = close().is_ok();
            }
            result
        } else {
            Ok(0)
        };
        let produced = process::wait(writer);
        let consumed = reader.and_then(process::wait);
        cancellation?;
        if test && (produced.is_ok() && process::wait(writer) != Err(rt::DENIED)
            || consumed.is_ok() && reader.is_ok_and(|key| process::wait(key) != Err(rt::DENIED)))
        {
            return Err(rt::REFUSED);
        }
        let (fault, number) = produced?;
        let writer_status = if fault { 128 } else { number };
        if test {
            write(tty::ERROR, &alloc::format!("[pipe cat={writer_status}]\n"))?;
        }
        let (fault, number) = consumed?;
        let reader_status = if fault { 128 } else { number };
        if test {
            write(tty::ERROR, &alloc::format!("[pipe wc={reader_status}]\n"))?;
        }
        Ok(reader_status)
    })();
    let closed = if closed_early { Ok(0) } else { close() };
    let released = grant.release(fs);
    closed?;
    released?;
    run
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
        // La sonde négative s'exécute dans un enfant privé du droit d'observation.
        // Le parent garde sa référence même si l'enfant termine avant la mesure.
        let sample_valid = if test && p == Program::Probe && words.as_slice() == ["droits"] {
            let mut sample = [0u64; 3];
            let count = rt::value(rt::call(
                tty::PROCESS_SAMPLE,
                1,
                sample.as_mut_ptr() as u64,
                tty::SAMPLE_BYTES as u64,
            ));
            count == Ok(1) && sample[0] != 0 && sample[0] != rt::call(rt::PROCESS_ID, 0, 0, 0).value
        } else {
            true
        };
        let (fault, code) = process::wait(key)?;
        if !sample_valid {
            return Err(rt::REFUSED);
        }
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
