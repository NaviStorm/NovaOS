use super::{
    diagnostic, engine,
    options::{self, Action},
    quoting::{Style, quote},
};
use alloc::vec::Vec;
use nova_cli::{console, documentation, process, runtime as rt, tty};

fn command() -> rt::Result<()> {
    let argv = process::arguments()?;
    let args: Vec<_> = argv.iter().map(|s| s.as_bytes()).collect();
    match options::parse(&args) {
        Err(problem) => {
            let message = diagnostic::message(problem, true);
            console::printable(
                tty::ERROR,
                &alloc::format!(
                    "basename: {}\n",
                    alloc::string::String::from_utf8_lossy(&message)
                ),
            )?;
            Err(rt::BAD_ARGUMENT)
        }
        Ok(Action::Names {
            paths,
            suffix,
            zero,
        }) => {
            // La seule sortie déléguée par cette ABI est un terminal texte.
            // Refuser le NUL avant le premier nom évite une sortie tronquée.
            if zero {
                console::write(
                    tty::ERROR,
                    "basename: NUL unsupported on NovaOS text console\n",
                )?;
                return Err(nova_cli::protocol::UNSUPPORTED);
            }
            for path in paths {
                let (leaf, _) = engine::record(path, suffix, zero);
                let quoted = quote(leaf, Style::ShellEscape, true);
                let s = core::str::from_utf8(&quoted).map_err(|_| rt::BAD_ARGUMENT)?;
                console::write(tty::OUTPUT, s)?;
                console::write(tty::OUTPUT, "\n")?;
            }
            Ok(())
        }
        Ok(doc) => console::document(
            documentation::lookup("basename").unwrap(),
            &argv,
            matches!(doc, Action::Help),
        ),
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(command().err().unwrap_or(0))
}
