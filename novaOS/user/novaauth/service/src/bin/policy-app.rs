#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "../../../../storage/runtime.rs"]
mod rt;
// Même ELF pour les deux utilisateurs. Aucun facteur de possession dans l'application.
fn serve() -> rt::Result<()> {
    loop {
        let cmd = rt::receive_for(rt::config(1), 15000)?;
        if cmd.len != 128 || cmd.kind != 1 {
            return Err(rt::BAD_ARGUMENT);
        }
        if cmd.payload[0] == 0 {
            return Ok(());
        }
        rt::send_frame(rt::config(9), cmd.tag, &cmd.payload, 0, false)?;
        // Le superviseur peut maintenant solliciter l'attestation sur l'autre canal.
        rt::send_for(rt::config(0), cmd.tag, &[1], 5000)?;
        let wait = if cmd.payload[0] == 7 { 1000 } else { 15000 };
        let mut r = [0; 128];
        match rt::receive_for(rt::config(10), wait) {
            Ok(v)
                if v.tag == cmd.tag
                    && v.len == 128
                    && v.kind == 1
                    && &v.payload[120..] == b"DIRREPLY" =>
            {
                r = v.payload
            }
            Err(rt::TIMED_OUT) if cmd.payload[0] == 7 => {
                r[..8].copy_from_slice(&files::UNCERTAIN.to_le_bytes())
            }
            _ => return Err(rt::REFUSED),
        }
        rt::send_for(rt::config(0), cmd.tag, &r, 5000)?;
    }
}
#[allow(dead_code)]
#[path = "../../../../../abi/files.rs"]
mod files;
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(serve().err().unwrap_or(0))
}
