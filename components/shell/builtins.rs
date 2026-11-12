use alloc::string::String;
use nova_cli::{fileapi::Client, protocol as w, runtime as rt};
pub const COMMANDS: &str = "[ alias break cat cd clear continue cp echo exit export false head help history ls mkdir more mv nsh ps pwd return rm rmdir tail test top touch true unalias unset vi wc";
pub fn console_action(action: u64) -> rt::Result<()> {
    use nova_cli::tty;
    rt::value(rt::call(tty::CONSOLE, tty::SCREEN.into(), action, 0)).map(|_| ())
}
pub fn cd(fs: &mut Client, args: &[String]) -> rt::Result<()> {
    let path = match args {
        [path] => path,
        [sep, path] if sep == "--" => path,
        _ => {
            return Err(rt::BAD_ARGUMENT);
        }
    };
    let path = super::paths::resolve(fs, path)?;
    let dir = fs.directory(0, path.as_bytes(), w::DIR_ALL)?;
    let changed = fs.simple(w::CHDIR, dir);
    let closed = fs.simple(w::DIR_CLOSE, dir);
    changed.and(closed).map(|_| ())
}

pub fn monitor(interactive: bool) -> rt::Result<()> {
    use nova_cli::{console, tty};
    if interactive {
        console_action(tty::ENTER_SCREEN)?;
    }
    let result = (|| loop {
        if interactive {
            rt::value(rt::call(
                tty::CONSOLE,
                tty::SCREEN.into(),
                tty::POSITION,
                (1 << 16) | 1,
            ))?;
        }
        console::write(tty::OUTPUT, "PID  ROLE  ETAT  MEMOIRE PRIVEE (Kio)\n")?;
        for index in 0..2 {
            let mut sample = [0u64; 3];
            let present = rt::value(rt::call(
                tty::PROCESS_SAMPLE,
                index,
                sample.as_mut_ptr() as u64,
                tty::SAMPLE_BYTES as u64,
            ))?;
            if present != 0 {
                let role = if index == 0 { "nsh" } else { "enfant" };
                let state = if sample[1] == 0 { "terminé" } else { "vivant" };
                if interactive {
                    console_action(tty::ERASE_EOL)?;
                }
                console::write(
                    tty::OUTPUT,
                    &alloc::format!("{}  {role}  {state}  {}\n", sample[0], sample[2] * 4),
                )?;
            }
        }
        if !interactive {
            return Ok(());
        }
        console_action(tty::ERASE_EOL)?;
        console::write(tty::OUTPUT, "q pour quitter")?;
        match rt::value(rt::call(
            tty::CONSOLE,
            tty::SCREEN.into(),
            tty::READ_TIMED,
            100,
        )) {
            Ok(113 | 27 | 3) => return Ok(()),
            Ok(_) | Err(rt::TIMED_OUT) => (),
            Err(e) => return Err(e),
        }
    })();
    if interactive {
        console_action(tty::LEAVE_SCREEN)?;
    }
    result
}
