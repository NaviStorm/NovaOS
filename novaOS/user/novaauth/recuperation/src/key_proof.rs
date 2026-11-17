//! Défi de possession des clés publiques de réception, lors du provisionnement.
use super::*;

const INFO: &[u8] = b"NovaAuth/recovery/receiver-key-pop/v1";

pub fn seal_key_proof(
    recipient: &[u8; 32],
    binding: &[u8; 32],
    challenge: &Secret,
    rng: &mut (impl RngCore + CryptoRng),
) -> Result<[u8; 80]> {
    let recipient =
        <Kem as KemTrait>::PublicKey::from_bytes(recipient).map_err(|_| Error::Crypto)?;
    let (enc, mut sender) =
        hpke::setup_sender::<AesGcm256, HkdfSha256, Kem, _>(&OpModeS::Base, &recipient, INFO, rng)
            .map_err(|_| Error::Crypto)?;
    let mut clear = Zeroizing::new(**challenge);
    let tag = sender
        .seal_in_place_detached(&mut clear[..], binding)
        .map_err(|_| Error::Crypto)?;
    let mut envelope = [0; 80];
    envelope[..32].copy_from_slice(&enc.to_bytes());
    envelope[32..64].copy_from_slice(&clear[..]);
    envelope[64..].copy_from_slice(&tag.to_bytes());
    Ok(envelope)
}

pub fn open_key_proof(key: &PrivateKey, binding: &[u8; 32], envelope: &[u8]) -> Result<[u8; 32]> {
    if envelope.len() != 80 {
        return Err(Error::Format);
    }
    let enc =
        <Kem as KemTrait>::EncappedKey::from_bytes(&envelope[..32]).map_err(|_| Error::Crypto)?;
    let mut receiver =
        hpke::setup_receiver::<AesGcm256, HkdfSha256, Kem>(&OpModeR::Base, key, &enc, INFO)
            .map_err(|_| Error::Crypto)?;
    let mut clear = Zeroizing::new([0; 32]);
    clear.copy_from_slice(&envelope[32..64]);
    let tag = AeadTag::<AesGcm256>::from_bytes(&envelope[64..]).map_err(|_| Error::Crypto)?;
    receiver
        .open_in_place_detached(&mut clear[..], binding, &tag)
        .map_err(|_| Error::Crypto)?;
    let mut result = Sha256::new();
    result.update(binding);
    result.update(&clear[..]);
    Ok(result.finalize().into())
}
