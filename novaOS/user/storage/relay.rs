#![no_std]
#![no_main]
mod runtime;
#[allow(dead_code)]
#[path = "../../abi/block.rs"]
mod wire;
use runtime::*;
fn serve() -> Result<()> {
    let (input, output) = channels()?;
    let lower = (config(2), config(3));
    let mut last = 0;
    loop {
        let m = receive_for(input, 5000)?;
        let valid = m.kind == 1 && m.tag > last && m.len == 128 && wire::syntax(&m.payload);
        if !valid {
            send(output, m.tag, &[BAD_ARGUMENT as u8])?;
            continue;
        }
        last = m.tag;
        send(lower.0, m.tag, &m.payload)?;
        let answer = receive(lower.1)?;
        if answer.kind != 1 || answer.tag != m.tag || !matches!(answer.len, 1 | 128) {
            return Err(BAD_ARGUMENT);
        }
        send(output, m.tag, &answer.payload[..answer.len as usize])?;
        if m.payload[1] == wire::CLOSE && answer.len == 128 {
            return Ok(());
        }
    }
}
// Sûreté : point d'entrée privé de l'image plate du service bloc.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(serve().err().unwrap_or(0))
}
