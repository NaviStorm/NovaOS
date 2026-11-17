use crate::{engine::*, quoting::Style};
use nova_cli::{console, documentation, process, runtime as rt, tty};
fn print(port: u8, data: &[u8]) -> rt::Result<()> {
    console::write(
        port,
        core::str::from_utf8(data).map_err(|_| rt::BAD_ARGUMENT)?,
    )
}
fn run() -> rt::Result<()> {
    let args = process::arguments()?;
    let inherited = Entries::new();
    let mut trace_status = Ok(());
    let mut trace = |s: &[u8]| {
        if trace_status.is_ok() {
            trace_status = print(tty::ERROR, s)
        }
    };
    let result = (|| {
        let plan = parse(
            args.iter().map(|s| s.as_bytes().to_vec()).collect(),
            None,
            &mut trace,
        )?;
        if let Some(help) = plan.doc {
            return Ok(console::document(
                documentation::lookup("env").unwrap(),
                &args,
                help,
            ));
        }
        if plan.debug && plan.empty {
            trace(b"clearing environment\n")
        }
        let entries = plan.environment(
            inherited,
            |_| {
                Err(Error::new(
                    b"no delegated byte input for --env0-from".to_vec(),
                ))
            },
            &mut trace,
        )?;
        plan.validate()?;
        if !plan.command.is_empty() {
            return Err(Error::new(b"child launch unavailable".to_vec()));
        }
        if !plan.empty {
            return Err(Error::new(b"inherited environment unavailable".to_vec()));
        }
        if plan.zero && !entries.is_empty() {
            return Err(Error::new(b"NUL requires byte output".to_vec()));
        }
        for e in &entries {
            let mut line = display(e, Style::ShellEscape, true);
            line.push(b'\n');
            if let Err(rc) = print(tty::OUTPUT, &line) {
                return Ok(Err(rc));
            }
        }
        Ok(Ok(()))
    })();
    trace_status?;
    match result {
        Ok(status) => status,
        Err(error) => {
            print(tty::ERROR, &cat(&[b"env: ", &error.message, b"\n"]))?;
            if error.shebang {
                print(
                    tty::ERROR,
                    b"env: use -[v]S to pass options in shebang lines\n",
                )?;
            }
            if error.usage {
                print(tty::ERROR, b"Try 'env --help' for more information.\n")?
            }
            Err(error.code.into())
        }
    }
}
#[unsafe(export_name = "run")]
extern "C" fn run_env() -> ! {
    rt::exit(run().err().unwrap_or(0))
}
