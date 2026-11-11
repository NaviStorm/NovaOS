#![no_std]
#![no_main]
mod runtime;
use runtime::*;

struct Session {
    requests: u64,
    replies: u64,
}
impl Session {
    fn exchange(&self, id: u64, bytes: &[u8], expected: u8) -> Result<()> {
        send(self.requests, id, bytes)?;
        let reply = receive(self.replies)?;
        if reply.kind == 1
            && reply.tag == id
            && reply.len == 12
            && reply.payload[..8] == [WIRE_VERSION, expected, 0, 0, 0, 0, 0, 0]
            && reply.payload[8..12] == *IDENTITY
        {
            Ok(())
        } else {
            Err(BAD_ARGUMENT)
        }
    }
}

fn message(op: u8) -> [u8; 8] {
    [WIRE_VERSION, op, 0, 0, 0, 0, 0, 0]
}

fn client() -> Result<()> {
    let (requests, replies) = channels()?;
    let s = Session { requests, replies };
    let count = config(1);
    let mut id = 1;
    if count == 0 {
        // Sondes exécutées dans le véritable processus client de qualification.
        if receive(requests).err() != Some(DENIED)
            || send(replies, 1, &[]).err() != Some(DENIED)
            || call(IPC_REVOKE, requests, 0, 0).status != DENIED
            || call(IPC_DUP, requests, SEND.into(), 0).status != DENIED
        {
            return Err(BAD_ARGUMENT);
        }
        s.exchange(0, &message(INFO), BAD_FRAME)?;
        s.exchange(id, &[WIRE_VERSION], BAD_FRAME)?;
        id += 1;
        for (bytes, status) in [
            ([2, INFO, 0, 0, 0, 0, 0, 0], BAD_FRAME),
            ([1, FIN, 1, 0, 0, 0, 0, 0], BAD_FRAME),
            ([1, INFO, 0, 0, 1, 0, 0, 0], NO_VOLUME),
            (message(99), UNSUPPORTED),
        ] {
            s.exchange(id, &bytes, status)?;
            id += 1;
        }
        s.exchange(id - 1, &message(FIN), BAD_FRAME)?;
    }
    for _ in 0..count.max(1) {
        s.exchange(id, &message(INFO), 0)?;
        id += 1;
    }
    s.exchange(id, &message(FIN), 0)
}

// Sûreté : point d'entrée Rust unique appelé par _start dans cette image.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(client().err().unwrap_or(0))
}
