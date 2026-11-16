//! Implanté uniquement dans l'export et le profil de qualification des politiques.
use super::*;
use sha2::{Digest, Sha256};
pub(super) fn image(id: u64) -> Option<&'static [u8]> {
    match id {
        18 => Some(include_bytes!(concat!(env!("OUT_DIR"), "/sign-alice.elf"))),
        19 => Some(include_bytes!(concat!(env!("OUT_DIR"), "/sign-bob.elf"))),
        21 | 22 => Some(include_bytes!(concat!(env!("OUT_DIR"), "/policy-app.elf"))),
        23 => Some(include_bytes!(concat!(
            env!("OUT_DIR"),
            "/policy-update.elf"
        ))),
        _ => auth_lab::elf(id),
    }
}
fn measure(bytes: &[u8]) -> [u64; 4] {
    let d = Sha256::digest(bytes);
    core::array::from_fn(|i| u64::from_le_bytes(d[i * 8..i * 8 + 8].try_into().unwrap()))
}
pub(super) fn publish_measure(s: &Core, owner: Pid) {
    config(s, owner, 20, &measure(image(21).unwrap()));
    config(s, owner, 24, &measure(image(23).unwrap()));
}
pub(super) fn verify(elf: &[u8], words: &[u64; 16]) -> Res<()> {
    if words[7..11] != measure(elf) || words[11..13] == [0; 2] || words[13..] != [0; 3] {
        return Err(DENIED);
    }
    Ok(())
}
pub(super) fn attach(
    s: &mut Core,
    m: Session,
    target: Pid,
    id: u64,
    install: [u64; 2],
    elf: &[u8],
) -> Res<()> {
    // Le processus est créé à partir de ce même slice immuable. Sa première instruction
    // suit la vérification, le rattachement des endpoints et la publication par start.
    let (tx, rx) = channel(s, target, Some(m.fs), SEND)?;
    let (receive, respond) = channel(s, target, Some(m.fs), RECEIVE)?;
    if let Some(space) = s.processes.get(target).space.as_ref() {
        let (pages, stack) = space.page_counts();
        hal::log(format_args!(
            "NAP memoire pid={} id={id} privees={pages} pile={stack} octets_page=4096",
            target.number
        ));
    }
    let lane = usize::from(id == 22);
    let mut attestation = [0; 6];
    attestation[..2].copy_from_slice(&install);
    attestation[2..].copy_from_slice(&measure(elf));
    config(s, m.fs, 32 + lane * 6, &attestation);
    config(s, m.fs, 24 + lane * 2, &[rx, respond]);
    config(s, target, 9, &[tx, receive]);
    Ok(())
}
