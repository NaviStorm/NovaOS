//! Requête privée canonique approuvée, 292 octets.
use super::*;

pub const REQUEST_BYTES: usize = 292;
const INFO: &[u8] = b"NovaAuth/recovery/request/v1";

pub fn seal_request(
    role: u8,
    context: &DeliveryContext,
    recipient: &[u8; 32],
    signing: &SigningKey,
    rng: &mut (impl RngCore + CryptoRng),
) -> Result<[u8; REQUEST_BYTES]> {
    let mut clear = context.body(role, &Zeroizing::new([0; 32]))?;
    let public = <Kem as KemTrait>::PublicKey::from_bytes(recipient).map_err(|_| Error::Crypto)?;
    let (encapsulation, mut sender) =
        hpke::setup_sender::<AesGcm256, HkdfSha256, Kem, _>(&OpModeS::Base, &public, INFO, rng)
            .map_err(|_| Error::Crypto)?;
    let mut envelope = [0; REQUEST_BYTES];
    envelope[..8].copy_from_slice(b"NASHRQ01");
    envelope[8] = 1;
    envelope[9] = role;
    envelope[10..42].copy_from_slice(&context.recovery);
    envelope[42..58].copy_from_slice(&context.delivery);
    envelope[58..90].copy_from_slice(&encapsulation.to_bytes());
    let tag = sender
        .seal_in_place_detached(&mut clear[..122], &envelope[..58])
        .map_err(|_| Error::Crypto)?;
    envelope[90..212].copy_from_slice(&clear[..122]);
    envelope[212..228].copy_from_slice(&tag.to_bytes());
    let signature = signing.sign(&envelope[..228]);
    envelope[228..].copy_from_slice(&signature.to_bytes());
    Ok(envelope)
}

/// L'appelant vérifie aussi l'approbation courante et la réservation durable.
pub fn open_request(
    role: u8,
    context: &DeliveryContext,
    key: &PrivateKey,
    sender: &VerifyingKey,
    envelope: &[u8],
) -> Result<()> {
    role_index(role)?;
    if envelope.len() != REQUEST_BYTES
        || &envelope[..8] != b"NASHRQ01"
        || envelope[8] != 1
        || envelope[9] != role
        || envelope[10..42] != context.recovery
        || envelope[42..58] != context.delivery
    {
        return Err(Error::Format);
    }
    let signature = Signature::from_slice(&envelope[228..]).map_err(|_| Error::Crypto)?;
    sender
        .verify_strict(&envelope[..228], &signature)
        .map_err(|_| Error::Crypto)?;
    let enc =
        <Kem as KemTrait>::EncappedKey::from_bytes(&envelope[58..90]).map_err(|_| Error::Crypto)?;
    let mut receiver =
        hpke::setup_receiver::<AesGcm256, HkdfSha256, Kem>(&OpModeR::Base, key, &enc, INFO)
            .map_err(|_| Error::Crypto)?;
    let mut clear = Zeroizing::new([0; 122]);
    clear.copy_from_slice(&envelope[90..212]);
    let tag = AeadTag::<AesGcm256>::from_bytes(&envelope[212..228]).map_err(|_| Error::Crypto)?;
    receiver
        .open_in_place_detached(&mut clear[..], &envelope[..58], &tag)
        .map_err(|_| Error::Crypto)?;
    let expected = context.body(role, &Zeroizing::new([0; 32]))?;
    if clear[..] != expected[..122] {
        return Err(Error::Scope);
    }
    Ok(())
}
