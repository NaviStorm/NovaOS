// Même ELF pour les deux utilisateurs. Aucun facteur de possession dans l'application.
fn serve() -> rt::Result<()> {
    loop {
        let cmd = match rt::receive_for(rt::config(1), 15000) {
            Ok(v) => v,
            Err(rt::TIMED_OUT) => continue,
            Err(e) => return Err(e),
        };
        if cmd.len != 128 || cmd.kind != 1 {
            return Err(rt::BAD_ARGUMENT);
        }
        if cmd.payload[0] == 0 {
            return Ok(());
        }
        rt::send_frame(rt::config(9), cmd.tag, &cmd.payload, 0, false)?;
        // Le superviseur peut maintenant solliciter l'attestation sur l'autre canal.
        rt::send_for(rt::config(0), cmd.tag, &[1], 5000)?;
        let wait = if cmd.payload[0] == 7 { 2000 } else { 15000 };
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
