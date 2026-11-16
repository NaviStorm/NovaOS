//! ELF de recette installé après le démarrage.
#![no_std]
#![no_main]
use nova_cli::runtime as rt;
use nova_exec_client::Launch;

fn prefix(launch: &Launch) -> rt::Result<()> {
    let input = match launch.argv.as_slice() {
        [] => b"-".as_slice(),
        [file] if !file.starts_with('-') || *file == "-" => file.as_bytes(),
        ["--", file] => file.as_bytes(),
        _ => return Err(rt::BAD_ARGUMENT),
    };
    launch.with_input(input, |read| {
        launch.write_stdout(b"T:")?;
        let mut buf = [0; 1024];
        loop {
            let n = read(&mut buf)?;
            if n == 0 {
                return Ok(());
            }
            launch.write_stdout(&buf[..n])?;
        }
    })
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let result = Launch::current().and_then(|launch| prefix(&launch));
    rt::exit(result.err().unwrap_or(0))
}
