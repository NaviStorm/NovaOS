#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#[cfg(not(target_os = "none"))]
compile_error!("Cet utilitaire est destiné à la cible native NovaOS.");

#[cfg(target_os = "none")]
mod guest {
    use nova_cli::{command::Program, documentation, options, runtime as rt};
    use nova_exec_client::Launch;

    fn copy(launch: &Launch) -> rt::Result<()> {
        let args = options::parse(Program::Cat, &launch.argv)?;
        if args.help || args.version {
            let page = documentation::lookup("cat").ok_or(rt::BAD_ARGUMENT)?;
            let lang = if launch.french {
                documentation::Language::French
            } else {
                documentation::Language::English
            };
            let text = if args.help {
                page.render(&[], false, lang).ok_or(rt::BAD_ARGUMENT)?
            } else {
                page.version(lang)
            };
            return launch.write_stdout(text.as_bytes());
        }
        if args.files.is_empty() {
            return Err(rt::BAD_ARGUMENT);
        }
        let mut data = [0; 1024];
        for file in args.files {
            launch.with_input(file.as_bytes(), |read| {
                loop {
                    let n = read(&mut data)?;
                    if n == 0 {
                        break Ok(());
                    }
                    launch.write_stdout(&data[..n])?;
                }
            })?;
        }
        Ok(())
    }
    #[unsafe(no_mangle)]
    extern "C" fn run() -> ! {
        let result = Launch::current().and_then(|launch| {
            let result = copy(&launch);
            if let Err(reason) = result {
                let _ = launch.write_stderr(nova_cli::console::reason(reason).as_bytes());
            }
            result
        });
        rt::exit(result.err().unwrap_or(0))
    }
}
