//! Réponse durable à une consultation fraîche. Tous les champs sont en ordre réseau.
use crate::{common, incarnation, runtime as rt};
use ed25519_dalek::{Signature, VerifyingKey};

pub const CURRENT: u64 = u64::from_le_bytes(*b"NACURR01");
pub const PROOF: u64 = u64::from_le_bytes(*b"NACURR02");
pub const PREFIX: &[u8] = b"NovaAuth/QEMU/etat-courant/1\0";

/// La preuve couvre le nonce choisi par le registre et tous les champs de Published.packet().
pub fn query(port: u64, local: &[u8; 32]) -> rt::Result<[u8;112]> {
    let fresh = common::random()?;
    let mut req = [0u8; 128];
    req[..32].copy_from_slice(local);
    req[32..64].copy_from_slice(&fresh[..]);
    rt::send_for(port, CURRENT, &req, 2000)?;
    let state = rt::receive_for(port, 12000)?;
    if state.kind != 1 || state.tag != CURRENT || state.len != 112 {
        return Err(rt::REFUSED);
    }
    let proof = rt::receive_for(port, 2000)?;
    if proof.kind != 1 || proof.tag != PROOF || proof.len != 64 {
        return Err(rt::REFUSED);
    }
    let record = &state.payload[..112];
    let mut signed = [0u8; PREFIX.len() + 32 + 112];
    signed[..PREFIX.len()].copy_from_slice(PREFIX);
    signed[PREFIX.len()..PREFIX.len()+32].copy_from_slice(&fresh[..]);
    signed[PREFIX.len()+32..].copy_from_slice(record);
    let key = VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB")))
        .map_err(|_| rt::REFUSED)?;
    let sig = Signature::from_slice(&proof.payload[..64]).map_err(|_|rt::REFUSED)?;
    key.verify_strict(&signed,&sig).map_err(|_|rt::REFUSED)?;
    if record[..16] != incarnation::SERVER || record[16..32] != incarnation::DOMAIN
        || record[80..112] == [0;32] || record[32..64] == [0;32] {
        return Err(rt::REFUSED);
    }
    common::marker(b"NAUTH tete signature et portee verifiees");
    if record[32..64] != local[..] {
        common::marker(b"NAUTH incarnation locale retiree");
        return Err(rt::DENIED);
    }
    Ok(record.try_into().map_err(|_| rt::REFUSED)?)
}
