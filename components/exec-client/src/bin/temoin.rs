//! Charge d'installation à chaud. Aucun accès au système de fichiers.
#![no_std]
#![no_main]

use nova_cli::runtime as rt;
use nova_exec_client::Launch;

fn emit(launch: &Launch) -> rt::Result<()> {
    if launch.argv.is_empty() {
        return launch.write_stdout(b"A\tB\tC\nD\tE\tF\n");
    }
    // Un préfixe de longueur préserve les arguments vides et les octets de contrôle.
    for arg in &launch.argv {
        let len = u16::try_from(arg.len()).map_err(|_| rt::LIMIT)?;
        launch.write_stdout(&len.to_le_bytes())?;
        launch.write_stdout(arg.as_bytes())?;
    }
    Ok(())
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(
        Launch::current()
            .and_then(|launch| emit(&launch))
            .err()
            .unwrap_or(0),
    )
}
