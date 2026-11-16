fn check(ok: bool) -> rt::Result<()> {
    ok.then_some(()).ok_or(rt::REFUSED)
}
// Seul le superviseur de qualification possède le canal de contrôle de ce client.
// Une preuve ou une attestation ne peut être fabriquée par le processus applicatif.
fn serve() -> rt::Result<()> {
    let mut client = Client {
        key: SigningKey::from_bytes(SEED),
        seq: 0,
    };
    let mut sid = [0; 32];
    loop {
        let cmd = rt::receive_for(rt::config(1), 15000)?;
        check(cmd.kind == 1 && cmd.len == 128)?;
        let op = cmd.payload[0];
        let mut r = [0; 128];
        match op {
            0 => return Ok(()),
            1 => {
                let (subject, s, _) = client.login()?;
                sid = s;
                r[16..32].copy_from_slice(&subject);
                r[32..64].copy_from_slice(&s);
            }
            3 | 4 | 11..=17 => {
                let mut q = client.frame(op);
                q[40..72].copy_from_slice(&sid);
                q[72..].copy_from_slice(&cmd.payload[72..]);
                r = client.send(&q)?;
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        r[112..120].copy_from_slice(&(client.seq + 1).to_le_bytes());
        rt::send_for(rt::config(0), cmd.tag, &r, 5000)?;
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(serve().err().unwrap_or(0))
}
