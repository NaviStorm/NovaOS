//! Encodages et cryptographie de la récupération QEMU approuvée.
//! Les autorisations courantes, réservations durables et routes privées sont
//! vérifiées par les acteurs avant l'appel. Une signature historique ne vaut
//! jamais admission ni autorisation d'activation.
#![no_std]
use aes_gcm::{Aes256Gcm, KeyInit, aead::AeadInPlace};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hkdf::Hkdf;
use hpke::{
    Deserializable, Kem as KemTrait, OpModeR, OpModeS, Serializable,
    aead::{AeadTag, AesGcm256, ExportOnlyAead},
    kdf::HkdfSha256,
    kem::X25519HkdfSha256,
};
use rand_core::{CryptoRng, RngCore};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

type Kem = X25519HkdfSha256;
pub type Secret = Zeroizing<[u8; 32]>;
pub type PrivateKey = <Kem as KemTrait>::PrivateKey;
pub const CONTEXT_BYTES: usize = 302;
pub const DESCRIPTOR_BYTES: usize = 386;
pub const MANIFEST_BYTES: usize = 418;
pub const DELIVERY_BYTES: usize = 324;
pub const CHUNK: usize = 1024;
pub const MAX_OBJECT: usize = 16384;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Format,
    Scope,
    Crypto,
    Missing,
    Duplicate,
    Order,
    Limit,
}
pub type Result<T> = core::result::Result<T, Error>;

pub fn hash(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}
fn u32_at(data: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(data[at..at + 4].try_into().unwrap())
}
fn u64_at(data: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(data[at..at + 8].try_into().unwrap())
}
fn role_index(role: u8) -> Result<usize> {
    match role {
        1 => Ok(0),
        2 => Ok(1),
        _ => Err(Error::Scope),
    }
}
pub fn public(key: &PrivateKey) -> [u8; 32] {
    Kem::sk_to_pk(key).to_bytes().into()
}
pub fn private(bytes: &[u8]) -> Result<PrivateKey> {
    PrivateKey::from_bytes(bytes).map_err(|_| Error::Format)
}
pub fn keypair(rng: &mut (impl RngCore + CryptoRng)) -> (PrivateKey, [u8; 32]) {
    let (key, public) = Kem::gen_keypair(rng);
    (key, public.to_bytes().into())
}

/// Tous les champs proviennent de la capture et de l'époque de clés épinglée.
/// Head est le format réseau natif de 144 octets, sans la magie NATRIP01.
pub struct Capture {
    pub server: [u8; 16],
    pub domain: [u8; 16],
    pub backup: [u8; 32],
    pub generation: u64,
    pub key_epoch: u64,
    pub recipients: [[u8; 32]; 2],
    pub head: [u8; 144],
}
impl Capture {
    fn context(&self) -> Result<[u8; CONTEXT_BYTES]> {
        if self.server == [0; 16]
            || self.domain == [0; 16]
            || self.backup == [0; 32]
            || self.generation == 0
            || self.key_epoch == 0
            || self.recipients[0] == self.recipients[1]
            || self.recipients.contains(&[0; 32])
            || self.head[..16] != self.domain
            || u64_at(&self.head, 88) != self.generation
            || self.head[112..] == [0; 32]
        {
            return Err(Error::Scope);
        }
        let mut c = [0; CONTEXT_BYTES];
        c[..8].copy_from_slice(b"NABACK01");
        c[8..14].copy_from_slice(&[0, 32, 0, 1, 255, 255]);
        c[14..30].copy_from_slice(&self.server);
        c[30..46].copy_from_slice(&self.domain);
        c[46..78].copy_from_slice(&self.backup);
        c[78..86].copy_from_slice(&self.generation.to_be_bytes());
        c[86..94].copy_from_slice(&self.key_epoch.to_be_bytes());
        c[94..126].copy_from_slice(&hash(&self.recipients[0]));
        c[126..158].copy_from_slice(&hash(&self.recipients[1]));
        c[158..].copy_from_slice(&self.head);
        Ok(c)
    }
}

const EXPORT_INFO: &[u8] = b"NovaAuth/backup/HPKE-info/v1\0";
fn export_info(context: &[u8], role: u8, key_id: &[u8]) -> [u8; EXPORT_INFO.len() + 65] {
    let mut info = [0; EXPORT_INFO.len() + 65];
    let n = EXPORT_INFO.len();
    info[..n].copy_from_slice(EXPORT_INFO);
    info[n..n + 32].copy_from_slice(&hash(context));
    info[n + 32] = role;
    info[n + 33..].copy_from_slice(key_id);
    info
}
const EXPORT: &[u8] = b"NovaAuth/backup/share/v1";
fn data_key(descriptor: &[u8], shares: &[Secret; 2]) -> Result<Secret> {
    let mut input = Zeroizing::new([0; 64]);
    input[..32].copy_from_slice(&shares[0][..]);
    input[32..].copy_from_slice(&shares[1][..]);
    let hkdf = Hkdf::<Sha256>::new(Some(&hash(descriptor)), &input[..]);
    let mut key = Zeroizing::new([0; 32]);
    hkdf.expand(b"NovaAuth/backup/data-key/v1", &mut key[..])
        .map_err(|_| Error::Crypto)?;
    Ok(key)
}

/// Création unique d'un chiffré. Aucune reprise d'un encodeur interrompu.
pub struct Encoder {
    descriptor: [u8; DESCRIPTOR_BYTES],
    key: Secret,
    next: u32,
    ciphertext: Sha256,
}
impl Encoder {
    pub fn new(
        capture: &Capture,
        lengths: [u32; 3],
        rng: &mut (impl RngCore + CryptoRng),
    ) -> Result<Self> {
        let context = capture.context()?;
        validate_lengths(lengths)?;
        let mut descriptor = [0; DESCRIPTOR_BYTES];
        descriptor[..CONTEXT_BYTES].copy_from_slice(&context);
        let mut shares = [Zeroizing::new([0; 32]), Zeroizing::new([0; 32])];
        for role in 1..=2 {
            let i = role_index(role)?;
            let pk = <Kem as KemTrait>::PublicKey::from_bytes(&capture.recipients[i])
                .map_err(|_| Error::Crypto)?;
            let info = export_info(&context, role, &context[94 + 32 * i..126 + 32 * i]);
            let (enc, ctx) = hpke::setup_sender::<ExportOnlyAead, HkdfSha256, Kem, _>(
                &OpModeS::Base,
                &pk,
                &info,
                rng,
            )
            .map_err(|_| Error::Crypto)?;
            ctx.export(EXPORT, &mut shares[i][..])
                .map_err(|_| Error::Crypto)?;
            descriptor[302 + 32 * i..334 + 32 * i].copy_from_slice(&enc.to_bytes());
        }
        for (i, n) in lengths.into_iter().enumerate() {
            descriptor[366 + 4 * i..370 + 4 * i].copy_from_slice(&n.to_be_bytes());
        }
        descriptor[378..382].copy_from_slice(&chunk_count(lengths).to_be_bytes());
        descriptor[382..386].copy_from_slice(&(CHUNK as u32).to_be_bytes());
        let key = data_key(&descriptor, &shares)?;
        Ok(Self {
            descriptor,
            key,
            next: 0,
            ciphertext: Sha256::new(),
        })
    }
    /// Chiffre exactement le prochain fragment global, sans réutiliser le nonce.
    pub fn seal(&mut self, clear: &mut [u8]) -> Result<[u8; 16]> {
        let (role, len) = position(&self.descriptor, self.next)?;
        if clear.len() != len {
            return Err(Error::Format);
        }
        let nonce = nonce(self.next);
        let aad = aad(&self.descriptor, role, self.next, len);
        // L'index est consommé avant l'AEAD, y compris en cas d'erreur.
        self.next += 1;
        let cipher = Aes256Gcm::new_from_slice(&self.key[..]).map_err(|_| Error::Crypto)?;
        let tag = cipher
            .encrypt_in_place_detached((&nonce).into(), &aad, clear)
            .map_err(|_| Error::Crypto)?;
        self.ciphertext.update(&*clear);
        self.ciphertext.update(tag);
        Ok(tag.into())
    }
    pub fn finish(self) -> Result<[u8; MANIFEST_BYTES]> {
        if self.next != u32_at(&self.descriptor, 378) {
            return Err(Error::Missing);
        }
        let mut m = [0; MANIFEST_BYTES];
        m[..DESCRIPTOR_BYTES].copy_from_slice(&self.descriptor);
        m[DESCRIPTOR_BYTES..].copy_from_slice(&self.ciphertext.finalize());
        Ok(m)
    }
}
fn validate_lengths(lengths: [u32; 3]) -> Result<()> {
    if lengths
        .into_iter()
        .any(|n| n == 0 || n as usize > MAX_OBJECT)
    {
        return Err(Error::Limit);
    }
    Ok(())
}
fn chunk_count(lengths: [u32; 3]) -> u32 {
    lengths.into_iter().map(|n| n.div_ceil(CHUNK as u32)).sum()
}
fn position(d: &[u8; DESCRIPTOR_BYTES], mut index: u32) -> Result<(u8, usize)> {
    for role in 1..=3 {
        let length = u32_at(d, 366 + 4 * (role as usize - 1));
        let chunks = length.div_ceil(CHUNK as u32);
        if index < chunks {
            return Ok((
                role,
                (length - index * CHUNK as u32).min(CHUNK as u32) as usize,
            ));
        }
        index -= chunks;
    }
    Err(Error::Order)
}
fn nonce(index: u32) -> [u8; 12] {
    let mut n = [0; 12];
    n[8..].copy_from_slice(&index.to_be_bytes());
    n
}
fn aad(d: &[u8; DESCRIPTOR_BYTES], role: u8, index: u32, len: usize) -> [u8; 45] {
    let mut a = [0; 45];
    a[..32].copy_from_slice(&hash(d));
    a[32] = role;
    a[33..37].copy_from_slice(&index.to_be_bytes());
    a[37..41].copy_from_slice(&(len as u32).to_be_bytes());
    a[41..].copy_from_slice(&d[378..382]);
    a
}

#[derive(Clone)]
pub struct Manifest([u8; MANIFEST_BYTES]);
impl Manifest {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let m: [u8; MANIFEST_BYTES] = bytes.try_into().map_err(|_| Error::Format)?;
        if &m[..8] != b"NABACK01"
            || m[8..14] != [0, 32, 0, 1, 255, 255]
            || m[14..30] == [0; 16]
            || m[30..46] == [0; 16]
            || m[46..78] == [0; 32]
            || u64_at(&m, 78) == 0
            || u64_at(&m, 86) == 0
            || m[94..126] == m[126..158]
            || m[158..174] != m[30..46]
            || m[246..254] != m[78..86]
            || m[270..302] == [0; 32]
            || m[302..334] == [0; 32]
            || m[334..366] == [0; 32]
            || u32_at(&m, 382) != CHUNK as u32
        {
            return Err(Error::Format);
        }
        let lengths = [u32_at(&m, 366), u32_at(&m, 370), u32_at(&m, 374)];
        validate_lengths(lengths)?;
        if u32_at(&m, 378) != chunk_count(lengths) {
            return Err(Error::Format);
        }
        Ok(Self(m))
    }
    pub fn bytes(&self) -> &[u8; MANIFEST_BYTES] {
        &self.0
    }
    pub fn digest(&self) -> [u8; 32] {
        hash(&self.0)
    }
    pub fn head(&self) -> &[u8] {
        &self.0[158..302]
    }
    pub fn epoch(&self) -> u64 {
        u64_at(&self.0, 86)
    }
    pub fn descriptor(&self) -> &[u8; DESCRIPTOR_BYTES] {
        self.0[..DESCRIPTOR_BYTES].try_into().unwrap()
    }
    /// Appelé séparément par chaque détenteur après réservation durable.
    pub fn share(&self, role: u8, key: &PrivateKey) -> Result<Secret> {
        let i = role_index(role)?;
        if hash(&public(key)) != self.0[94 + 32 * i..126 + 32 * i] {
            return Err(Error::Scope);
        }
        let enc = <Kem as KemTrait>::EncappedKey::from_bytes(&self.0[302 + 32 * i..334 + 32 * i])
            .map_err(|_| Error::Crypto)?;
        let info = export_info(
            &self.0[..CONTEXT_BYTES],
            role,
            &self.0[94 + 32 * i..126 + 32 * i],
        );
        let ctx = hpke::setup_receiver::<ExportOnlyAead, HkdfSha256, Kem>(
            &OpModeR::Base,
            key,
            &enc,
            &info,
        )
        .map_err(|_| Error::Crypto)?;
        let mut share = Zeroizing::new([0; 32]);
        ctx.export(EXPORT, &mut share[..])
            .map_err(|_| Error::Crypto)?;
        Ok(share)
    }
}

/// L'appelant possède deux remises authentifiées pour la même opération.
/// Chaque fragment reste privé jusqu'à la validation intégrale ET l'activation.
pub struct Decoder {
    manifest: Manifest,
    key: Secret,
    next: u32,
    ciphertext: Sha256,
    failed: bool,
}
impl Decoder {
    pub fn new(manifest: Manifest, shares: &[Secret; 2]) -> Result<Self> {
        let key = data_key(manifest.descriptor(), shares)?;
        Ok(Self {
            manifest,
            key,
            next: 0,
            ciphertext: Sha256::new(),
            failed: false,
        })
    }
    pub fn open(&mut self, cipher: &mut [u8], tag: &[u8; 16]) -> Result<()> {
        if self.failed {
            cipher.zeroize();
            return Err(Error::Crypto);
        }
        let result = self.open_next(cipher, tag);
        if result.is_err() {
            self.failed = true;
            cipher.zeroize();
            self.key.zeroize();
        }
        result
    }
    fn open_next(&mut self, cipher: &mut [u8], tag: &[u8; 16]) -> Result<()> {
        let (role, len) = position(self.manifest.descriptor(), self.next)?;
        if cipher.len() != len {
            return Err(Error::Format);
        }
        self.ciphertext.update(&*cipher);
        self.ciphertext.update(tag);
        let nonce = nonce(self.next);
        let aad = aad(self.manifest.descriptor(), role, self.next, len);
        self.next += 1;
        let aead = Aes256Gcm::new_from_slice(&self.key[..]).map_err(|_| Error::Crypto)?;
        aead.decrypt_in_place_detached((&nonce).into(), &aad, cipher, tag.into())
            .map_err(|_| Error::Crypto)
    }
    pub fn finish(self) -> Result<()> {
        if self.failed || self.next != u32_at(self.manifest.bytes(), 378) {
            return Err(Error::Missing);
        }
        if self.ciphertext.finalize().as_slice() != &self.manifest.bytes()[DESCRIPTOR_BYTES..] {
            return Err(Error::Crypto);
        }
        Ok(())
    }
}

const DELIVERY_INFO: &[u8] = b"NovaAuth/recovery/delivery/v1";
pub struct DeliveryContext {
    pub recovery: [u8; 32],
    pub manifest: [u8; 32],
    pub epoch: u64,
    pub delivery: [u8; 16],
    pub nonce: [u8; 32],
}
impl DeliveryContext {
    fn body(&self, role: u8, share: &Secret) -> Result<Zeroizing<[u8; 154]>> {
        role_index(role)?;
        if self.recovery == [0; 32]
            || self.manifest == [0; 32]
            || self.epoch == 0
            || self.delivery == [0; 16]
            || self.nonce == [0; 32]
        {
            return Err(Error::Scope);
        }
        let mut b = Zeroizing::new([0; 154]);
        b[0] = 1;
        b[1] = role;
        b[2..34].copy_from_slice(&self.recovery);
        b[34..66].copy_from_slice(&self.manifest);
        b[66..74].copy_from_slice(&self.epoch.to_be_bytes());
        b[74..90].copy_from_slice(&self.delivery);
        b[90..122].copy_from_slice(&self.nonce);
        b[122..].copy_from_slice(&share[..]);
        Ok(b)
    }
}

/// Le détenteur conserve ce chiffré exact avant toute émission. La reprise
/// explicite retourne ces mêmes octets au destinataire encore autorisé.
pub fn seal_share(
    role: u8,
    context: &DeliveryContext,
    share: &Secret,
    recipient: &[u8; 32],
    signing: &SigningKey,
    rng: &mut (impl RngCore + CryptoRng),
) -> Result<[u8; DELIVERY_BYTES]> {
    let mut body = context.body(role, share)?;
    let pk = <Kem as KemTrait>::PublicKey::from_bytes(recipient).map_err(|_| Error::Crypto)?;
    let (enc, mut hpke) = hpke::setup_sender::<AesGcm256, HkdfSha256, Kem, _>(
        &OpModeS::Base,
        &pk,
        DELIVERY_INFO,
        rng,
    )
    .map_err(|_| Error::Crypto)?;
    let mut out = [0; DELIVERY_BYTES];
    out[..8].copy_from_slice(b"NASHAR01");
    out[8] = 1;
    out[9] = role;
    out[10..42].copy_from_slice(&context.recovery);
    out[42..58].copy_from_slice(&context.delivery);
    out[58..90].copy_from_slice(&enc.to_bytes());
    let tag = hpke
        .seal_in_place_detached(&mut body[..], &out[..58])
        .map_err(|_| Error::Crypto)?;
    out[90..244].copy_from_slice(&body[..]);
    out[244..260].copy_from_slice(&tag.to_bytes());
    let signature = signing.sign(&out[..260]);
    out[260..].copy_from_slice(&signature.to_bytes());
    Ok(out)
}
pub fn open_share(
    role: u8,
    context: &DeliveryContext,
    key: &PrivateKey,
    sender: &VerifyingKey,
    envelope: &[u8],
) -> Result<Secret> {
    role_index(role)?;
    if envelope.len() != DELIVERY_BYTES
        || &envelope[..8] != b"NASHAR01"
        || envelope[8] != 1
        || envelope[9] != role
        || envelope[10..42] != context.recovery
        || envelope[42..58] != context.delivery
    {
        return Err(Error::Scope);
    }
    let signature = Signature::from_slice(&envelope[260..]).map_err(|_| Error::Crypto)?;
    sender
        .verify_strict(&envelope[..260], &signature)
        .map_err(|_| Error::Crypto)?;
    let enc =
        <Kem as KemTrait>::EncappedKey::from_bytes(&envelope[58..90]).map_err(|_| Error::Crypto)?;
    let mut receiver = hpke::setup_receiver::<AesGcm256, HkdfSha256, Kem>(
        &OpModeR::Base,
        key,
        &enc,
        DELIVERY_INFO,
    )
    .map_err(|_| Error::Crypto)?;
    let mut body = Zeroizing::new([0; 154]);
    body.copy_from_slice(&envelope[90..244]);
    let tag = AeadTag::<AesGcm256>::from_bytes(&envelope[244..260]).map_err(|_| Error::Crypto)?;
    receiver
        .open_in_place_detached(&mut body[..], &envelope[..58], &tag)
        .map_err(|_| Error::Crypto)?;
    let expected = context.body(role, &Zeroizing::new([0; 32]))?;
    if body[..122] != expected[..122] {
        return Err(Error::Scope);
    }
    let mut share = Zeroizing::new([0; 32]);
    share.copy_from_slice(&body[122..]);
    Ok(share)
}

/// Deux emplacements distincts, pas de combinaison depuis une remise isolée.
/// Le contrôle courant de la route et de la tête précède take().
pub struct Shares {
    values: [Option<Secret>; 2],
    digests: [Option<[u8; 32]>; 2],
    scope: Option<([u8; 32], [u8; 32], u64, [u8; 32])>,
}
impl Default for Shares {
    fn default() -> Self {
        Self {
            values: [None, None],
            digests: [None, None],
            scope: None,
        }
    }
}
impl Shares {
    pub fn receive(
        &mut self,
        role: u8,
        context: &DeliveryContext,
        key: &PrivateKey,
        sender: &VerifyingKey,
        envelope: &[u8],
    ) -> Result<bool> {
        let i = role_index(role)?;
        let scope = (
            context.recovery,
            context.manifest,
            context.epoch,
            public(key),
        );
        if self.scope.is_some_and(|s| s != scope) {
            return Err(Error::Scope);
        }
        let share = open_share(role, context, key, sender, envelope)?;
        let digest = hash(envelope);
        if let Some(previous) = self.digests[i] {
            return if previous == digest {
                Ok(false)
            } else {
                Err(Error::Duplicate)
            };
        }
        self.scope = Some(scope);
        self.values[i] = Some(share);
        self.digests[i] = Some(digest);
        Ok(true)
    }
    pub fn take(mut self) -> Result<[Secret; 2]> {
        Ok([
            self.values[0].take().ok_or(Error::Missing)?,
            self.values[1].take().ok_or(Error::Missing)?,
        ])
    }
}

mod context_v2;
mod request;
pub use context_v2::{RECOVERY_V2_BYTES, RecoveryContextV2, SharesV2};
pub use request::{REQUEST_BYTES, open_request, seal_request};

mod key_proof;
pub use key_proof::{open_key_proof, seal_key_proof};

pub mod framing;
pub mod activation;

// Les primitives reçoivent explicitement leur CryptoRng. Les helpers implicites
// de dépendances ne doivent jamais trouver une source de substitution.
#[cfg(target_os = "none")]
fn refuse_implicit_random(_: &mut [u8]) -> core::result::Result<(), getrandom::Error> {
    Err(getrandom::Error::UNSUPPORTED)
}
#[cfg(target_os = "none")]
getrandom::register_custom_getrandom!(refuse_implicit_random);

pub mod exec_admission;

#[cfg(feature = "administration-lab")]
pub mod subject_admission;
