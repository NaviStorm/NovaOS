//! Commande privée de l'acteur d'équité. Chaque couple vérifie le rang et
//! les 128 octets de chaque requête/réponse. La notification termine le dialogue.
use super::*;

pub(super) fn travail(role: u64) -> Reply {
    let (tx, rx) = (read(6), read(7));
    let mut tours = 0u64;
    let mut calcul = 1u64;
    loop {
        if role != 1 && read(10) != 0 {
            if role == 0 {
                let fin = call(IPC_NOTIFY, tx, 1, 0);
                if fin.status != OK {
                    return fin;
                }
            }
            break;
        }
        let rang = tours + 1;
        if role <= 1 {
            if role == 0 {
                write(34, rang);
                let r = call(IPC_SEND, tx, SEND_BUF, 160);
                if r.status != OK || r.value != 128 {
                    return erreur();
                }
            }
            let r = call(IPC_RECEIVE, rx, RECV_BUF, 184);
            if role == 1 && r.status == OK && r.value == 2 && read(70) == 1 {
                break;
            }
            if r.status != OK
                || r.value != 1
                || read(67) != (128u64 << 32 | 1)
                || read(68) != rang
                || (0..16).any(|i| read(71 + i) != read(36 + i))
            {
                return erreur();
            }
            if role == 1 {
                write(34, rang);
                let r = call(IPC_SEND, tx, SEND_BUF, 160);
                if r.status != OK || r.value != 128 {
                    return erreur();
                }
            }
        } else if role == 2 {
            // La file reste exactement pleine pendant toute la fenêtre.
            if call(IPC_SEND, tx, SEND_BUF, 160).status != WOULD_BLOCK {
                return erreur();
            }
            call(THREAD_YIELD, 0, 0, 0);
        } else {
            for _ in 0..1024 {
                calcul =
                    core::hint::black_box(calcul.wrapping_mul(6364136223846793005).wrapping_add(1));
            }
        }
        tours = rang;
        write(11, tours);
    }
    Reply {
        status: OK,
        value: tours,
    }
}

fn erreur() -> Reply {
    Reply {
        status: BAD_ARGUMENT,
        value: 0,
    }
}
