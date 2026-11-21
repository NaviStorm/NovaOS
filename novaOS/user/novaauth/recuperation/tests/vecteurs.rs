//! Vecteur officiel RFC 9180 (provenance et empreinte dans rfc9180.json).
use hpke::{
    Deserializable, Kem, OpModeR, aead::ExportOnlyAead, kdf::HkdfSha256, kem::X25519HkdfSha256,
};
fn hex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(core::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn export_only_rfc9180_trois_contextes() {
    type K = X25519HkdfSha256;
    let key = <K as Kem>::PrivateKey::from_bytes(&hex(
        "33d196c830a12f9ac65d6e565a590d80f04ee9b19c83c87f2c170d972a812848",
    ))
    .unwrap();
    let enc = <K as Kem>::EncappedKey::from_bytes(&hex(
        "e5e8f9bfff6c2f29791fc351d2c25ce1299aa5eaca78a757c0b4fb4bcd830918",
    ))
    .unwrap();
    let receiver = hpke::setup_receiver::<ExportOnlyAead, HkdfSha256, K>(
        &OpModeR::Base,
        &key,
        &enc,
        b"Ode on a Grecian Urn",
    )
    .unwrap();
    for (ctx, expected) in [
        (
            "",
            "7a36221bd56d50fb51ee65edfd98d06a23c4dc87085aa5866cb7087244bd2a36",
        ),
        (
            "00",
            "d5535b87099c6c3ce80dc112a2671c6ec8e811a2f284f948cec6dd1708ee33f0",
        ),
        (
            "54657374436f6e74657874",
            "ffaabc85a776136ca0c378e5d084c9140ab552b78f039d2e8775f26efff4c70e",
        ),
    ] {
        let mut out = [0; 32];
        receiver.export(&hex(ctx), &mut out).unwrap();
        assert_eq!(out.as_slice(), hex(expected));
    }
}
