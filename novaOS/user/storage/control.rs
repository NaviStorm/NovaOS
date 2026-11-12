#![no_std]
#![no_main]
mod runtime;
use runtime::*;
fn probe() -> Result<()> {
    let (tx, rx) = channels()?;
    let mode = config(1);
    // Seule l'interface de session du service NovaFS est visible ici.
    for (id, op) in [(1, INFO), (2, FIN)] {
        send(tx, id, &[WIRE_VERSION, op, 0, 0, 0, 0, 0, 0])?;
        let r = loop {
            match receive(rx) {
                Err(TIMED_OUT) if mode == 43 && op == INFO => continue,
                result => break result?,
            }
        };
        if r.kind != 1
            || r.len != 12
            || r.tag != id
            || r.payload[0] != 1
            || &r.payload[8..12] != IDENTITY
        {
            return Err(BAD_ARGUMENT);
        }
        if r.payload[1] != 0 {
            return Err(r.payload[1] as u64);
        }
        if op == INFO && r.payload[4] != 1 {
            return Err(REFUSED);
        }
    }
    if mode == 0 {
        return Err(BAD_ARGUMENT);
    }
    Ok(())
}
// Sûreté : point d'entrée de l'application de diagnostic sans droit bloc.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(probe().err().unwrap_or(0))
}
