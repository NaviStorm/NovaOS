//! Témoin R1 de qualification, pas une commande d'administration distribuée.
#![no_std]
#![no_main]
use nova_cli::runtime as rt;
use nova_exec_client::admin::{AdministrativeLaunch, Conversation};
fn perform() -> rt::Result<()> {
    let launch = AdministrativeLaunch::current()?;
    let mut request = [0; 128];
    request[..8].copy_from_slice(b"NXACMD01");
    let mut repeats = 1;
    match launch.streams.argv.as_slice() {
        [] | ["--self"] => request[8]=1,
        ["--twice"] => {request[8]=1;repeats=2;},
        ["--user", name] | ["--group", name] => {
            if name.is_empty() || name.len()>32 {return Err(rt::BAD_ARGUMENT);}
            request[8]=if launch.streams.argv[0]=="--user" {2}else{3};
            request[32]=2;request[33]=name.len() as u8;
            request[40..40+name.len()].copy_from_slice(name.as_bytes());
        },
        ["--version"] => return launch.streams.write_stdout(b"admin-sonde 0.24.0 QEMU NXADMIN1\n"),
        ["--help"] | ["man"] => return launch.streams.write_stdout(if launch.streams.french {
            b"admin-sonde [--self|--twice|--user NOM|--group NOM]\nSonde QEMU. Statuts natifs ; aucune mutation.\n"
        } else {b"admin-sonde [--self|--twice|--user NAME|--group NAME]\nQEMU probe. Native statuses; no mutation.\n"}),
        _ => return Err(rt::BAD_ARGUMENT),
    }
    let mut conversation = Conversation::new(&launch);
    for _ in 0..repeats {
        let reply = conversation.exchange(&request)?;
        // Vue identité de 64 octets, distincte du rendu terminal. La trame
        // complète de 128 octets a été validée avant cet extrait.
        launch.streams.write_stdout(&reply[..64])?;
        let status = u64::from_le_bytes(reply[..8].try_into().unwrap());
        if status != 0 {
            return Err(status);
        }
    }
    Ok(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(perform().err().unwrap_or(0))
}
