//! Unité immuable P7 de deux blocs. L'empreinte de la paire est engagée par
//! son parent. Le lecteur fournit la position et le rôle attendus depuis
//! ce parent, y compris pour les unités publiques.
use crate::crypto::{Entropy, Secret};
use crate::{BLOCK_BYTES, Block, Error, Result, compression};
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

const MAGIC: &[u8; 8] = b"NP7UNIT1";
const VERSION: u16 = 1;
const AAD: usize = 88;
const NONCE_END: usize = AAD + 24;
const TAG_END: usize = NONCE_END + 16;
// En-tête fixe et seize entrées de quatre octets du format de pack P6.
const PACK_DIRECTORY: u32 = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    Data = 1,
    Metadata = 2,
    Packed = 3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Context {
    pub pool: [u8; 16],
    pub domain: [u8; 16],
    /// LBA physique du premier bloc de la paire, immuable après publication.
    pub physical: u64,
    /// LBA d'une page ou base (multiple de 64) de la feuille d'un pack.
    pub logical: u64,
    pub generation: u64,
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    /// Occupation originale dans la feuille de 64 LBA, y compris les entrées
    /// dont une vue particulière a déjà divergé.
    pub mask: u64,
    pub logical_len: u32,
    /// Octets significatifs avant bourrage. Inclut le répertoire pour LZ4.
    pub encoded_len: u32,
    pub encrypted: bool,
}

pub struct Keys {
    domain: [u8; 16],
    data: Secret,
    metadata: Secret,
    pub(crate) view: Secret,
}

impl Keys {
    pub fn derive(master: &Secret, domain: &[u8; 16]) -> Result<Self> {
        if *domain == [0; 16] {
            return Err(Error::Invalid("domaine chiffré P7 nul"));
        }
        Ok(Self {
            domain: *domain,
            view: master.derive(domain, b"vue/manifeste/v1")?,
            data: master.derive(domain, b"unite/donnees/v1")?,
            metadata: master.derive(domain, b"unite/metadonnees/v1")?,
        })
    }

    fn key(&self, ctx: Context) -> Result<&Secret> {
        if self.domain != ctx.domain {
            return Err(Error::Corrupt("clé d'un autre domaine P7"));
        }
        Ok(match ctx.kind {
            Kind::Metadata => &self.metadata,
            Kind::Data | Kind::Packed => &self.data,
        })
    }
}

impl Context {
    fn validate(self) -> Result<()> {
        if self.pool == [0; 16]
            || self.physical.checked_add(1).is_none()
            || self.generation == 0
            || (self.kind == Kind::Packed && (self.logical >= 1024 || self.logical % 64 != 0))
        {
            return Err(Error::Corrupt("contexte d'unité P7"));
        }
        Ok(())
    }
}

impl Descriptor {
    fn from_plain(ctx: Context, plain: &Block, encrypted: bool) -> Result<Self> {
        let mut d = Self {
            mask: 1 << (ctx.logical % 64),
            logical_len: BLOCK_BYTES as u32,
            encoded_len: BLOCK_BYTES as u32,
            encrypted,
        };
        if ctx.kind == Kind::Packed {
            let unit = compression::inspect(plain)?;
            d.mask = 0;
            for lba in unit.lbas().map(u64::from) {
                if lba / 64 != ctx.logical / 64 {
                    return Err(Error::Corrupt("pack P7 déplacé entre feuilles"));
                }
                d.mask |= 1 << (lba % 64);
            }
            d.logical_len = d.mask.count_ones() * BLOCK_BYTES as u32;
            d.encoded_len = PACK_DIRECTORY + unit.encoded_bytes() as u32;
        }
        Ok(d)
    }

    fn validate(self, ctx: Context) -> Result<()> {
        let page = BLOCK_BYTES as u32;
        let valid = match ctx.kind {
            Kind::Data | Kind::Metadata => {
                self.mask == 1 << (ctx.logical % 64)
                    && self.logical_len == page
                    && self.encoded_len == page
            }
            Kind::Packed => {
                let count = self.mask.count_ones();
                (2..=16).contains(&count)
                    && self.logical_len == count * page
                    && (PACK_DIRECTORY + count..=page).contains(&self.encoded_len)
            }
        };
        if valid && (!self.encrypted || ctx.domain != [0; 16]) {
            Ok(())
        } else {
            Err(Error::Corrupt("longueur, domaine ou portée d'unité P7"))
        }
    }
}

fn header(ctx: Context, d: Descriptor) -> Block {
    let mut h = [0; BLOCK_BYTES];
    h[..8].copy_from_slice(MAGIC);
    h[8..10].copy_from_slice(&VERSION.to_le_bytes());
    h[10] = u8::from(d.encrypted);
    h[11] = ctx.kind as u8;
    h[12] = u8::from(ctx.kind == Kind::Packed);
    h[16..32].copy_from_slice(&ctx.pool);
    h[32..48].copy_from_slice(&ctx.domain);
    h[48..56].copy_from_slice(&ctx.logical.to_le_bytes());
    h[56..64].copy_from_slice(&ctx.physical.to_le_bytes());
    h[64..72].copy_from_slice(&ctx.generation.to_le_bytes());
    h[72..76].copy_from_slice(&d.logical_len.to_le_bytes());
    h[76..80].copy_from_slice(&d.encoded_len.to_le_bytes());
    h[80..88].copy_from_slice(&d.mask.to_le_bytes());
    h
}

fn parse(h: &Block) -> Result<(Context, Descriptor)> {
    let kind = match h[11] {
        1 => Kind::Data,
        2 => Kind::Metadata,
        3 => Kind::Packed,
        _ => return Err(Error::Corrupt("type d'unité P7 inconnu")),
    };
    if &h[..8] != MAGIC
        || h[8..10] != VERSION.to_le_bytes()
        || h[10] > 1
        || h[12] != u8::from(kind == Kind::Packed)
        || h[13..16] != [0; 3]
        || h[TAG_END..].iter().any(|&b| b != 0)
        || (h[10] == 0 && h[AAD..TAG_END].iter().any(|&b| b != 0))
    {
        return Err(Error::Corrupt("profil ou bourrage d'unité P7"));
    }
    let n = |p| u64::from_le_bytes(h[p..p + 8].try_into().expect("champ fixe u64"));
    let len = |p| u32::from_le_bytes(h[p..p + 4].try_into().expect("champ fixe u32"));
    let ctx = Context {
        pool: h[16..32].try_into().expect("identifiant fixe"),
        domain: h[32..48].try_into().expect("identifiant fixe"),
        logical: n(48),
        physical: n(56),
        generation: n(64),
        kind,
    };
    let d = Descriptor {
        mask: n(80),
        logical_len: len(72),
        encoded_len: len(76),
        encrypted: h[10] == 1,
    };
    ctx.validate()?;
    d.validate(ctx)?;
    Ok((ctx, d))
}

/// Lecture structurelle du contexte public. Le graphe doit confronter ces
/// champs à la référence entrante avant de leur faire confiance.
pub fn context(header: &Block) -> Result<Context> {
    Ok(parse(header)?.0)
}

pub fn fingerprint(header: &Block, payload: &Block) -> [u8; 32] {
    Sha256::new()
        .chain_update(b"NovaFS/P7/paire/v1")
        .chain_update(header)
        .chain_update(payload)
        .finalize()
        .into()
}

pub fn seal(
    ctx: Context,
    keys: Option<&Keys>,
    rng: &mut (impl Entropy + ?Sized),
    plain: &Block,
) -> Result<(Block, Block, [u8; 32])> {
    ctx.validate()?;
    let descriptor = Descriptor::from_plain(ctx, plain, keys.is_some())?;
    descriptor.validate(ctx)?;
    let mut header = header(ctx, descriptor);
    let mut payload = Zeroizing::new(*plain);
    if let Some(keys) = keys {
        let (nonce, tag) = keys.key(ctx)?.seal(rng, &header[..AAD], &mut payload[..])?;
        header[AAD..NONCE_END].copy_from_slice(&nonce);
        header[NONCE_END..TAG_END].copy_from_slice(&tag);
    }
    let hash = fingerprint(&header, &payload);
    Ok((header, *payload, hash))
}

/// Compare une empreinte externe et contrôle la forme publique. Un résultat
/// structurel sur une paire chiffrée laisse son tag et son contenu à vérifier.
pub fn inspect(
    ctx: Context,
    header: &Block,
    payload: &Block,
    expected: &[u8; 32],
) -> Result<Descriptor> {
    let (actual, descriptor) = parse(header)?;
    if actual != ctx || fingerprint(header, payload) != *expected {
        return Err(Error::Corrupt("substitution de paire P7"));
    }
    Ok(descriptor)
}

pub fn open(
    ctx: Context,
    keys: Option<&Keys>,
    header: &Block,
    payload: &Block,
    expected: &[u8; 32],
) -> Result<Block> {
    let descriptor = inspect(ctx, header, payload, expected)?;
    match (descriptor.encrypted, keys.is_some()) {
        (true, false) => return Err(Error::Unavailable),
        (false, true) => return Err(Error::Corrupt("déclassement d'une unité chiffrée P7")),
        _ => {}
    }
    let mut plain = Zeroizing::new(*payload);
    if let Some(keys) = keys {
        keys.key(ctx)?.open(
            header[AAD..NONCE_END].try_into().expect("nonce fixe"),
            header[NONCE_END..TAG_END].try_into().expect("tag fixe"),
            &header[..AAD],
            &mut plain[..],
        )?;
    }
    // L'interprétation du répertoire LZ4 requiert d'abord la vérification du
    // tag. Un bloc brut arbitraire n'entre jamais dans le parseur de packs.
    if Descriptor::from_plain(ctx, &plain, descriptor.encrypted)? != descriptor {
        return Err(Error::Corrupt(
            "répertoire différent de la portée authentifiée P7",
        ));
    }
    Ok(*plain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::{Entropy, HostEntropy};

    fn base(kind: Kind) -> Context {
        Context {
            pool: [1; 16],
            domain: [2; 16],
            physical: 99,
            logical: 64,
            generation: 11,
            kind,
        }
    }

    #[test]
    fn substitutions_et_nonce_apres_copie() -> Result<()> {
        let ctx = base(Kind::Data);
        let keys = Keys::derive(&Secret::from_bytes([42; 32]), &ctx.domain)?;
        let plain = [0x45; BLOCK_BYTES];
        let (h, p, digest) = seal(ctx, Some(&keys), &mut HostEntropy, &plain)?;
        assert_ne!(p, plain);
        assert_eq!(open(ctx, Some(&keys), &h, &p, &digest)?, plain);
        let (second, _, _) = seal(ctx, Some(&keys), &mut HostEntropy, &plain)?;
        assert_ne!(h[AAD..NONCE_END], second[AAD..NONCE_END]);
        assert!(matches!(
            open(ctx, None, &h, &p, &digest),
            Err(Error::Unavailable)
        ));
        for other in [
            Context {
                pool: [3; 16],
                ..ctx
            },
            Context {
                domain: [4; 16],
                ..ctx
            },
            Context {
                logical: ctx.logical + 64,
                ..ctx
            },
            Context {
                physical: 100,
                ..ctx
            },
            Context {
                generation: 12,
                ..ctx
            },
            Context {
                kind: Kind::Metadata,
                ..ctx
            },
        ] {
            assert!(open(other, Some(&keys), &h, &p, &digest).is_err());
            // Attaquant qui refabrique aussi le descripteur et l'empreinte publique.
            let mut forged = h;
            let d = inspect(ctx, &h, &p, &digest)?;
            forged[..AAD].copy_from_slice(&header(other, d)[..AAD]);
            assert!(open(other, Some(&keys), &forged, &p, &fingerprint(&forged, &p)).is_err());
        }
        for offset in [0, AAD, NONCE_END, TAG_END, BLOCK_BYTES - 1] {
            let mut bad = h;
            bad[offset] ^= 1;
            assert!(open(ctx, Some(&keys), &bad, &p, &fingerprint(&bad, &p)).is_err());
        }
        let mut swapped = p;
        swapped[800] ^= 1;
        assert!(open(ctx, Some(&keys), &h, &swapped, &digest).is_err());
        assert!(open(ctx, Some(&keys), &h, &swapped, &fingerprint(&h, &swapped)).is_err());
        Ok(())
    }

    #[test]
    fn packed_apres_authentification_et_meta_independante() -> Result<()> {
        let ctx = base(Kind::Packed);
        let keys = Keys::derive(&Secret::from_bytes([42; 32]), &ctx.domain)?;
        let mut pack = compression::Packer::default();
        assert_eq!(pack.push(66, &[12; BLOCK_BYTES])?, compression::Push::Added);
        assert_eq!(pack.push(78, &[17; BLOCK_BYTES])?, compression::Push::Added);
        let packed = pack.finish().unwrap();
        let (h, p, hash) = seal(ctx, Some(&keys), &mut HostEntropy, &packed)?;
        let d = inspect(ctx, &h, &p, &hash)?;
        assert!(d.encrypted && d.encoded_len < 200);
        assert_eq!(d.mask, (1 << 2) | (1 << 14));
        assert_eq!(d.logical_len, 8192);
        let restored = open(ctx, Some(&keys), &h, &p, &hash)?;
        assert_eq!(
            compression::inspect(&restored)?.page(78)?,
            [17; BLOCK_BYTES]
        );
        for pos in [12, 72, 76, 80] {
            let mut bad = h;
            bad[pos] ^= 1;
            assert!(open(ctx, Some(&keys), &bad, &p, &fingerprint(&bad, &p)).is_err());
        }
        // Un re-scellement administrativement erroné conserve malgré un tag valide
        // un désaccord entre le masque public et le répertoire protégé.
        let mut fake = header(
            ctx,
            Descriptor {
                mask: (1 << 3) | (1 << 14),
                ..d
            },
        );
        let mut payload = packed;
        let (nonce, tag) = keys
            .data
            .seal(&mut HostEntropy, &fake[..AAD], &mut payload)?;
        fake[AAD..NONCE_END].copy_from_slice(&nonce);
        fake[NONCE_END..TAG_END].copy_from_slice(&tag);
        assert!(
            open(
                ctx,
                Some(&keys),
                &fake,
                &payload,
                &fingerprint(&fake, &payload)
            )
            .is_err()
        );
        let meta = base(Kind::Metadata);
        let (mh, mp, hash) = seal(meta, Some(&keys), &mut HostEntropy, &packed)?;
        assert_eq!(open(meta, Some(&keys), &mh, &mp, &hash)?, packed);
        let mut untrusted = mp;
        assert!(
            keys.data
                .open(
                    mh[AAD..NONCE_END].try_into().unwrap(),
                    mh[NONCE_END..TAG_END].try_into().unwrap(),
                    &mh[..AAD],
                    &mut untrusted
                )
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn chemin_brut_clair_et_incompressible() -> Result<()> {
        let ctx = base(Kind::Data);
        let keys = Keys::derive(&Secret::from_bytes([42; 32]), &ctx.domain)?;
        let mut pack = compression::Packer::default();
        let mut incompressible = [0; BLOCK_BYTES];
        HostEntropy.fill(&mut incompressible)?;
        assert_eq!(pack.push(64, &incompressible)?, compression::Push::Raw);
        // Même la signature exacte d'un pack P6 doit rester des données ordinaires.
        incompressible[..8].copy_from_slice(b"NP6LZ4PK");
        for mode in [None, Some(&keys)] {
            let (h, p, hash) = seal(ctx, mode, &mut HostEntropy, &incompressible)?;
            assert_eq!(context(&h)?, ctx);
            assert_eq!(open(ctx, mode, &h, &p, &hash)?, incompressible);
            if mode.is_none() {
                assert_eq!(p, incompressible);
                assert!(open(ctx, Some(&keys), &h, &p, &hash).is_err());
                let mut malformed = h;
                malformed[AAD] = 1;
                assert!(context(&malformed).is_err());
            }
        }
        Ok(())
    }
}
