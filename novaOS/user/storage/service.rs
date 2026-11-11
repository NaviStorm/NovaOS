#![no_std]
#![no_main]
mod runtime;
use runtime::*;

fn serve() -> Result<()> {
    let (requests, replies) = channels()?;
    let mut last = 0;
    loop {
        let msg = match receive_for(requests, if last == 0 { 15_000 } else { TIMEOUT }) {
            Ok(msg) => msg,
            Err(TIMED_OUT) if last == 0 => continue,
            Err(CLOSED | DENIED) => return Ok(()),
            Err(status) => return Err(status),
        };
        if msg.kind != 1 {
            // Une notification ne contient ni trame ni identifiant de requête.
            continue;
        }
        let p = &msg.payload;
        let mut finish = false;
        let status = if msg.tag <= last || msg.len != 8 {
            BAD_FRAME
        } else {
            last = msg.tag;
            if p[0] != WIRE_VERSION || p[2..4] != [0; 2] {
                BAD_FRAME
            } else if p[4..8] != [0; 4] {
                NO_VOLUME
            } else {
                match p[1] {
                    INFO => 0,
                    FIN => {
                        finish = true;
                        0
                    }
                    _ => UNSUPPORTED,
                }
            }
        };
        let mut answer = [0; 12];
        answer[0] = WIRE_VERSION;
        answer[1] = status;
        answer[8..].copy_from_slice(IDENTITY);
        send(replies, msg.tag, &answer)?;
        if finish {
            last = 0;
        }
    }
}

// Sûreté : point d'entrée Rust unique appelé par _start dans cette image.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(serve().err().unwrap_or(0))
}
