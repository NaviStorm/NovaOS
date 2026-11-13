#![no_std]
#![no_main]
mod runtime;
#[allow(dead_code)]
#[path = "../../abi/block.rs"]
mod wire;
use crate::runtime::bulk;
use runtime::*;
#[repr(align(8))]
struct Request(wire::Wire);
fn driver() -> Result<()> {
    let (input, output) = channels()?;
    let key = config(2);
    let mut geometry = Request(wire::blank(wire::INFO));
    value(call(wire::PORTAL, key, (&raw mut geometry) as u64, 128))?;
    let size = wire::number(&geometry.0, 4);
    if !matches!(size, 512 | 4096) {
        return Err(BAD_ARGUMENT);
    }
    let qualification = config(1) == 1;
    if qualification {
        if key > 1 && call(wire::PORTAL, key - 1, (&raw mut geometry) as u64, 128).status != DENIED
        {
            return Err(BAD_ARGUMENT);
        }
        let mut good = wire::blank(wire::READ);
        if !describe(&mut good, size) {
            return Err(BAD_ARGUMENT);
        }
        for (index, byte) in [
            (48, 255),
            (50, 0),
            (49, 5),
            (55, 255),
            (61, 255),
            (59, 1),
            (70, 3),
            (80, 1),
        ] {
            let mut bad = Request(good);
            bad.0[index] = byte;
            if call(wire::PORTAL, key, (&raw mut bad) as u64, 128).status != BAD_ARGUMENT {
                return Err(REFUSED);
            }
        }
    }
    let lane = config(10);
    let page = if lane != 0 {
        let base = value(call(HEAP_RESIZE, 0, 0, 0))?;
        value(call(
            HEAP_RESIZE,
            base,
            base.checked_add(4096).ok_or(LIMIT)?,
            0,
        ))?;
        // Sûreté : le pilote vient de réserver cette page privée, qu'il conserve seul.
        Some(unsafe { &mut *(base as *mut [u8; 4096]) })
    } else {
        None
    };
    let mut page = page;
    let mut last = 0;
    loop {
        let m = match receive_for(input, 5000) {
            Err(TIMED_OUT) if matches!(config(1), 41 | 42) => continue,
            r => r?,
        };
        if m.len != 128 || m.kind != 1 || m.tag <= last || !wire::syntax(&m.payload) {
            send(output, m.tag, &[BAD_ARGUMENT as u8])?;
            continue;
        }
        last = m.tag;
        let mut r = Request(m.payload);
        if r.0[2] == wire::PAGE_PHASE {
            let result: Result<()> = (|| {
                let data = page.as_deref_mut().ok_or(DENIED)?;
                let write = r.0[1] == wire::WRITE;
                if write {
                    bulk::take(lane, m.tag, data)?;
                }
                value(call(
                    wire::PAGE_PORTAL,
                    key,
                    (&raw const r) as u64,
                    data.as_mut_ptr() as u64,
                ))?;
                if !write {
                    bulk::put(lane, m.tag, data)?;
                }
                Ok(())
            })();
            match result {
                Ok(()) => send(output, m.tag, &r.0)?,
                Err(e) => send(output, m.tag, &[e as u8])?,
            }
            continue;
        }
        if matches!(
            (r.0[1], r.0[2]),
            (wire::READ, wire::BEGIN) | (wire::WRITE, wire::END) | (wire::FLUSH, wire::BEGIN)
        ) {
            if !describe(&mut r.0, size) {
                send(output, m.tag, &[BAD_ARGUMENT as u8])?;
                continue;
            }
        }
        let result = call(wire::PORTAL, key, (&raw mut r) as u64, 128);
        if result.status != OK {
            send(output, m.tag, &[result.status as u8])?;
            continue;
        }
        if result.value != 128 {
            return Err(BAD_ARGUMENT);
        }
        if matches!(config(1), 1 | 2) && (m.payload[1], m.payload[2]) == (wire::READ, wire::BEGIN) {
            // Même adresse utilisateur que le port vient de copier, y compris
            // pendant la lecture retenue du banc. Aucun octet de la table DMA n'y réside.
            r.0[32..].fill(0xe5);
            core::hint::black_box(&r);
            r.0[32..].fill(0);
        }
        if qualification && (m.payload[1], m.payload[2]) == (wire::READ, wire::BEGIN) {
            // La copie du pilote peut changer après la soumission. Seuls les trois
            // descripteurs privés du composant ont été publiés au périphérique.
            let mut second = Request(m.payload);
            describe(&mut second.0, size);
            if call(wire::PORTAL, key, (&raw mut second) as u64, 128).status != BAD_ARGUMENT {
                return Err(REFUSED);
            }
            second.0[32..].fill(0xe5);
            core::hint::black_box(&second);
        }
        if config(1) == 2 && (m.payload[1], m.payload[2]) == (wire::READ, wire::BEGIN) {
            // Panne confinée après publication d'une lecture valide, avant collecte.
            // Sûreté : sonde dans l'image utilisateur de qualification. L'instruction
            // termine ce seul processus via la porte d'exception ; aucune donnée n'est adressée.
            #[cfg(target_arch = "x86_64")]
            unsafe {
                core::arch::asm!("ud2", options(noreturn, nomem, nostack))
            }
            #[cfg(target_arch = "aarch64")]
            unsafe {
                core::arch::asm!("brk #0", options(noreturn, nomem, nostack))
            }
        }
        send(output, m.tag, &r.0)?;
        if m.payload[1] == wire::CLOSE {
            return Ok(());
        }
    }
}
// Sûreté : point d'entrée privé de l'image plate du pilote.
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(driver().err().unwrap_or(0))
}

// Le pilote utilisateur produit le header VirtIO little-endian et le sens des
// trois objets de la chaîne. Les identifiants locaux ne sont pas des adresses.
fn describe(w: &mut wire::Wire, size: u32) -> bool {
    let kind = match w[1] {
        wire::READ => 0,
        wire::WRITE => 1,
        wire::FLUSH => 4,
        _ => return false,
    };
    let Some(lba) = wire::lba(w).checked_mul(u64::from(size / 512)) else {
        return false;
    };
    wire::put(w, 32, kind);
    w[40..48].copy_from_slice(&lba.to_le_bytes());
    let mut i = 48;
    let mut seg = |obj, flags, next, len| {
        w[i..i + 3].copy_from_slice(&[obj, flags, next]);
        wire::put(w, i + 4, len);
        i += 8;
    };
    seg(1, 1, 1, 16);
    match kind {
        0 => seg(2, 3, 2, size),
        1 => seg(4, 1, 2, size),
        _ => (),
    }
    seg(3, 2, 0, 1);
    true
}
