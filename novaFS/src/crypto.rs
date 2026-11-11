//! Primitives P7 et deux emplacements de déverrouillage. Les implémentations
//! cryptographiques viennent de RustCrypto. Seul l'adaptateur HostEntropy
//! dépend du système hôte. Les clés ne s'affichent pas et ne se clonent pas.
extern crate alloc;

use self::alloc::{boxed::Box, vec::Vec};
use crate::{Error, Result};
use argon2::{Algorithm, Argon2, Block as ArgonBlock, Params, Version};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, aead::AeadInOut};
use core::sync::atomic::{AtomicBool, Ordering};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2_p7::Sha256;
use zeroize::{Zeroize, Zeroizing};

/// Remplir intégralement depuis un CSPRNG indépendant de l'image ou échouer.
/// Un backend natif doit renouveler son entropie après reprise d'un hôte cloné.
pub trait Entropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<()>;
}

#[cfg(not(target_os = "none"))]
pub struct HostEntropy;

#[cfg(not(target_os = "none"))]
impl Entropy for HostEntropy {
    fn fill(&mut self, out: &mut [u8]) -> Result<()> {
        getrandom::fill(out).map_err(|_| Error::Unavailable)
    }
}

const AUTHENTICATION: &str = "authentification P7 refusée";

/// Un seul propriétaire du tampon fixe. Déplacer le handle laisse les octets
/// à la même adresse. Les copies d'un appelant restent sous sa responsabilité.
pub struct Secret(Box<Zeroizing<[u8; 32]>>);

impl Secret {
    fn empty() -> Self {
        Self(Box::new(Zeroizing::new([0; 32])))
    }

    pub fn generate(rng: &mut (impl Entropy + ?Sized)) -> Result<Self> {
        let mut key = Self::empty();
        rng.fill(&mut key.0[..])?;
        Ok(key)
    }

    pub fn from_bytes(mut bytes: [u8; 32]) -> Self {
        let mut key = Self::empty();
        key.0.copy_from_slice(&bytes);
        bytes.zeroize();
        key
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// HKDF-Extract(domain, secret), Expand avec le préfixe fixe, la longueur
    /// sur un octet puis le libellé de protocole. Une sous-clé par usage.
    pub fn derive(&self, domain: &[u8; 16], purpose: &[u8]) -> Result<Self> {
        let n = u8::try_from(purpose.len()).map_err(|_| Error::Invalid("usage HKDF P7"))?;
        if n == 0 {
            return Err(Error::Invalid("usage HKDF P7 vide"));
        }
        let mut derived = Self::empty();
        let (mut prk, hkdf) = Hkdf::<Sha256>::extract(Some(domain), self.as_bytes());
        prk.as_mut_slice().zeroize();
        hkdf.expand_multi_info(&[b"NovaFS/P7/HKDF/v1", &[n], purpose], &mut derived.0[..])
            .map_err(|_| Error::Invalid("longueur HKDF P7"))?;
        Ok(derived)
    }

    /// Un nouveau tirage pour chaque appel, même si l'essai précédent a échoué.
    pub fn seal(
        &self,
        rng: &mut (impl Entropy + ?Sized),
        aad: &[u8],
        data: &mut [u8],
    ) -> Result<([u8; 24], [u8; 16])> {
        let mut nonce = [0; 24];
        rng.fill(&mut nonce)?;
        let cipher = XChaCha20Poly1305::new(self.as_bytes().into());
        let tag = cipher
            .encrypt_inout_detached((&nonce).into(), aad, data.into())
            .map_err(|_| Error::Limit("longueur de l'unité AEAD P7"))?;
        Ok((nonce, tag.into()))
    }

    /// RustCrypto vérifie le tag avant d'appliquer le flot de déchiffrement.
    /// Tout appel rejeté efface le tampon sans publier de contenu déchiffré.
    pub fn open(
        &self,
        nonce: &[u8; 24],
        tag: &[u8; 16],
        aad: &[u8],
        data: &mut [u8],
    ) -> Result<()> {
        let cipher = XChaCha20Poly1305::new(self.as_bytes().into());
        cipher
            .decrypt_inout_detached(nonce.into(), aad, (&mut *data).into(), tag.into())
            .map_err(|_| {
                data.zeroize();
                Error::Corrupt(AUTHENTICATION)
            })
    }

    fn hmac(&self, parts: &[&[u8]]) -> Hmac<Sha256> {
        let mut mac = <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(self.as_bytes())
            .expect("une clé de 32 octets est valide pour HMAC");
        for part in parts {
            mac.update(part);
        }
        mac
    }

    /// Le protocole appelant fournit des champs de longueur fixe ou préfixée.
    pub fn mac(&self, parts: &[&[u8]]) -> [u8; 32] {
        self.hmac(parts).finalize().into_bytes().into()
    }

    pub fn verify_mac(&self, parts: &[&[u8]], expected: &[u8; 32]) -> Result<()> {
        self.hmac(parts)
            .verify_slice(expected)
            .map_err(|_| Error::Corrupt(AUTHENTICATION))
    }
}

const MEMORY_KIB: u32 = 65_536;
const PASSES: u32 = 3;
const LANES: u32 = 4;
const MAX_PASSWORD: usize = 1024;
static KDF_BUSY: AtomicBool = AtomicBool::new(false);

struct KdfGuard;
impl Drop for KdfGuard {
    fn drop(&mut self) {
        KDF_BUSY.store(false, Ordering::Release);
    }
}

fn password_key(phrase: &[u8], salt: &[u8]) -> Result<Secret> {
    if phrase.is_empty() || phrase.len() > MAX_PASSWORD || salt.len() != 16 {
        return Err(Error::Invalid("longueur du secret de déverrouillage"));
    }
    KDF_BUSY
        .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        .map_err(|_| Error::Busy)?;
    let _guard = KdfGuard;
    let params = Params::new(MEMORY_KIB, PASSES, LANES, Some(32))
        .map_err(|_| Error::Invalid("profil Argon2id P7"))?;
    let mut memory = Zeroizing::new(Vec::<ArgonBlock>::new());
    memory
        .try_reserve_exact(MEMORY_KIB as usize)
        .map_err(|_| Error::NoMemory)?;
    memory.resize(MEMORY_KIB as usize, ArgonBlock::new());
    let mut output = Secret::empty();
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into_with_memory(phrase, salt, &mut output.0[..], memory.as_mut_slice())
        .map_err(|_| Error::Invalid("dérivation Argon2id P7"))?;
    Ok(output)
}

pub enum Credential<'a> {
    Passphrase(&'a [u8]),
    Recovery(&'a Secret),
}

const PROFILE: &[u8; 16] = b"NP7SLOT1\x13\x04\x03\x01\x00\x00\x01\x00";
const HEADER: usize = 32;
const ENVELOPE: usize = 72;
pub const SLOTS_BYTES: usize = HEADER + 2 * ENVELOPE;

/// Le profil est exact avant tout calcul Argon2. Les deux emplacements
/// contiennent chacun nonce[24], tag[16], clé maîtresse enveloppée[32].
#[derive(Clone)]
pub struct Slots([u8; SLOTS_BYTES]);

impl Slots {
    pub fn create(
        master: &Secret,
        domain: &[u8; 16],
        phrase: &[u8],
        recovery: &Secret,
        rng: &mut (impl Entropy + ?Sized),
    ) -> Result<Self> {
        let mut slots = Self([0; SLOTS_BYTES]);
        slots.0[..PROFILE.len()].copy_from_slice(PROFILE);
        rng.fill(&mut slots.0[16..HEADER])?;
        for credential in [
            Credential::Passphrase(phrase),
            Credential::Recovery(recovery),
        ] {
            let (index, key) = slots.wrapping_key(domain, credential)?;
            let mut wrapped = Zeroizing::new(*master.as_bytes());
            let (nonce, tag) = key.seal(rng, &slots.aad(domain, index), &mut wrapped[..])?;
            let start = HEADER + ENVELOPE * index;
            slots.0[start..start + 24].copy_from_slice(&nonce);
            slots.0[start + 24..start + 40].copy_from_slice(&tag);
            slots.0[start + 40..start + ENVELOPE].copy_from_slice(&wrapped[..]);
        }
        Ok(slots)
    }

    pub fn decode(raw: &[u8]) -> Result<Self> {
        if raw.len() != SLOTS_BYTES || &raw[..PROFILE.len()] != PROFILE {
            return Err(Error::Corrupt("profil des emplacements P7"));
        }
        Ok(Self(
            raw.try_into().expect("taille des emplacements vérifiée"),
        ))
    }

    pub fn as_bytes(&self) -> &[u8; SLOTS_BYTES] {
        &self.0
    }

    fn aad(&self, domain: &[u8; 16], index: usize) -> [u8; 57] {
        let mut aad = [0; 57];
        aad[..8].copy_from_slice(b"NP7WRAP1");
        aad[8..24].copy_from_slice(domain);
        aad[24..56].copy_from_slice(&self.0[..HEADER]);
        aad[56] = index as u8;
        aad
    }

    fn wrapping_key(
        &self,
        domain: &[u8; 16],
        credential: Credential<'_>,
    ) -> Result<(usize, Secret)> {
        match credential {
            Credential::Passphrase(p) => Ok((
                0,
                password_key(p, &self.0[16..HEADER])?.derive(domain, b"wrap-passphrase")?,
            )),
            Credential::Recovery(k) => Ok((1, k.derive(domain, b"wrap-recovery")?)),
        }
    }

    pub fn unlock(&self, domain: &[u8; 16], credential: Credential<'_>) -> Result<Secret> {
        let (index, key) = self.wrapping_key(domain, credential)?;
        let aad = self.aad(domain, index);
        let start = HEADER + index * ENVELOPE;
        let e = &self.0[start..start + ENVELOPE];
        let mut master = Secret::empty();
        master.0.copy_from_slice(&e[40..]);
        key.open(
            e[..24].try_into().expect("nonce fixe"),
            e[24..40].try_into().expect("tag fixe"),
            &aad,
            &mut master.0[..],
        )?;
        Ok(master)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::array;

    // Vecteurs publics uniquement. L'API hôte de production utilise HostEntropy.
    struct Fixture;
    impl Entropy for Fixture {
        fn fill(&mut self, out: &mut [u8]) -> Result<()> {
            for (i, b) in out.iter_mut().enumerate() {
                *b = i as u8;
            }
            Ok(())
        }
    }
    fn bytes<const N: usize>(s: &str) -> [u8; N] {
        assert_eq!(s.len(), 2 * N);
        array::from_fn(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
    }

    #[test]
    fn vecteur_xchacha_libsodium_et_rejets() -> Result<()> {
        // Oracle libsodium 1.0.22, crypto_aead_xchacha20poly1305_ietf_encrypt.
        let key = Secret::from_bytes(array::from_fn(|i| i as u8));
        let message = *b"NovaFS P7 test";
        let aad = b"NovaFS/AAD/v1";
        let mut page = message;
        let (nonce, tag) = key.seal(&mut Fixture, aad, &mut page)?;
        assert_eq!(nonce, array::from_fn(|i| i as u8));
        assert_eq!(page, bytes("d0ad791ed681adfe046452abb826"));
        assert_eq!(tag, bytes("cfe6e86243aedd22bbb3f1412268dfa7"));
        for field in 0..4 {
            let mut rejected = page;
            let mut n = nonce;
            let mut t = tag;
            let mut context = *aad;
            match field {
                0 => n[4] ^= 1,
                1 => t[15] ^= 1,
                2 => context[3] ^= 1,
                _ => rejected[0] ^= 1,
            }
            assert!(key.open(&n, &t, &context, &mut rejected).is_err());
            assert_eq!(rejected, [0; 14]);
        }
        key.open(&nonce, &tag, aad, &mut page)?;
        assert_eq!(page, message);
        Ok(())
    }

    #[test]
    fn vecteurs_hkdf_hmac_et_domaines() -> Result<()> {
        // HMAC-SHA256 et HKDF-Expand calculés indépendamment avec Python hmac.
        let key = Secret::from_bytes(array::from_fn(|i| i as u8));
        let domain = array::from_fn(|i| i as u8);
        let other = [42; 16];
        let data = key.derive(&domain, b"payload")?;
        assert_eq!(
            *data.as_bytes(),
            bytes("786ad66de4d4c3b0b8b2dd9dcd0bcc10c1b008431bd9e8418c0b2e3372866c75")
        );
        assert_ne!(
            data.as_bytes(),
            key.derive(&domain, b"metadata")?.as_bytes()
        );
        assert_ne!(data.as_bytes(), key.derive(&other, b"payload")?.as_bytes());
        let mac = key.mac(&[b"public-", b"metadata"]);
        assert_eq!(
            mac,
            bytes("cbdc3268918296d13c5485493dce99791ab591b9217562da2af45cdff23281c0")
        );
        assert!(key.verify_mac(&[b"private-metadata"], &mac).is_err());
        key.verify_mac(&[b"public-metadata"], &mac)
    }

    #[test]
    fn deux_emplacements_et_vecteur_argon_openssl() -> Result<()> {
        let salt: [u8; 16] = array::from_fn(|i| i as u8);
        let phrase = b"novafs-p7-corpus";
        // OpenSSL 3.5.9 KDF ARGON2ID, m=65536, t=3, p=4, clé de 32 octets.
        assert_eq!(
            *password_key(phrase, &salt)?.as_bytes(),
            bytes("aec7471995448630466682eb4f9e95d06f6d80527d56cb739b2c6554cff1486e")
        );
        let recovery = Secret::from_bytes([23; 32]);
        let master = Secret::from_bytes([91; 32]);
        let domain = [17; 16];
        let slots = Slots::create(&master, &domain, phrase, &recovery, &mut Fixture)?;
        assert!(
            !slots
                .as_bytes()
                .windows(32)
                .any(|w| w == master.as_bytes() || w == recovery.as_bytes())
        );
        let slots = Slots::decode(slots.as_bytes())?;
        for cred in [
            Credential::Passphrase(phrase),
            Credential::Recovery(&recovery),
        ] {
            assert_eq!(slots.unlock(&domain, cred)?.as_bytes(), master.as_bytes());
        }
        assert!(
            slots
                .unlock(&domain, Credential::Passphrase(b"incorrect"))
                .is_err()
        );
        assert!(
            slots
                .unlock(&[1; 16], Credential::Recovery(&recovery))
                .is_err()
        );
        let mut invalid = *slots.as_bytes();
        invalid[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(Slots::decode(&invalid).is_err());
        assert!(Slots::decode(&invalid[..175]).is_err());
        let mut wrong_tag = *slots.as_bytes();
        wrong_tag[HEADER + ENVELOPE + 24] ^= 1;
        assert!(
            Slots::decode(&wrong_tag)?
                .unlock(&domain, Credential::Recovery(&recovery))
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn echec_entropie_et_nonces_hote() -> Result<()> {
        struct Indisponible;
        impl Entropy for Indisponible {
            fn fill(&mut self, _: &mut [u8]) -> Result<()> {
                Err(Error::Unavailable)
            }
        }
        let key = Secret::generate(&mut HostEntropy)?;
        let mut page = *b"abc";
        assert!(key.seal(&mut Indisponible, b"domaine", &mut page).is_err());
        assert_eq!(&page, b"abc");
        let mut vus = std::collections::BTreeSet::new();
        for _ in 0..256 {
            let (nonce, tag) = key.seal(&mut HostEntropy, b"domaine", &mut page)?;
            assert!(vus.insert(nonce));
            key.open(&nonce, &tag, b"domaine", &mut page)?;
        }
        assert_eq!(&page, b"abc");
        Ok(())
    }
}
