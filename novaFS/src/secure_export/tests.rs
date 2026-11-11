//! Le banc du transport utilise des vues P6 déjà déverrouillées. La façade
//! P7 teste séparément les domaines de destination et les clés des volumes.
use super::*;
use crate::crypto::HostEntropy;
use crate::device::SimDevice;
use crate::shared::ObjectKind;
use crate::{Compression, Ids};
use std::io;

const OWNER: [u8; 16] = [39; 16];
const DOMAIN: [u8; 16] = [71; 16];
const IDS: Ids = Ids {
    fs: [3; 16],
    lv: [17; 16],
};
const START: usize = PREFIX + 24 + 16 + 32 + 24 + 16 + MANIFEST;
const ROW: usize = 8 + 24 + 16 + BLOCK_BYTES;
const PRIVATE: &[u8] = b"marqueur-de-confidentialite-du-transport-NovaFS";

fn source(sector: u32) -> Result<(SharedPool<SimDevice>, u64)> {
    let mut p = SharedPool::format_p6(SimDevice::new(2048, sector)?, [12; 16])?;
    let origin = p.create(OWNER, IDS, 256)?;
    p.create_file(origin, "/dossier-prive")?;
    p.write_file(origin, "/dossier-prive", 4101, PRIVATE)?;
    let snapshot = p.snapshot(origin)?;
    let clone = p.clone_snapshot(snapshot, OWNER)?;
    p.remove(origin)?;
    p.remove(snapshot)?;
    Ok((p, clone))
}

#[test]
fn restauration_apres_disparition_de_la_source() -> Result<()> {
    let mut rng = HostEntropy;
    let backup = Secret::generate(&mut rng)?;
    for sector in [512, 4096] {
        let (info, bytes) = {
            let (p, clone) = source(sector)?;
            let mut bytes = Vec::new();
            let info = export_view(&p, clone, sector, DOMAIN, &backup, &mut rng, &mut bytes)?;
            let mut second = Vec::new();
            assert_eq!(
                info,
                export_view(&p, clone, sector, DOMAIN, &backup, &mut rng, &mut second)?
            );
            assert_ne!(&bytes[16..32], &second[16..32]);
            assert_ne!(
                &bytes[START + 8..START + ROW],
                &second[START + 8..START + ROW]
            );
            (info, bytes)
        };
        for secret in [PRIVATE, &b"/dossier-prive"[..], backup.as_bytes(), &DOMAIN] {
            assert!(!bytes.windows(secret.len()).any(|w| w == secret));
        }
        assert_eq!(bytes.len(), START + info.pages as usize * ROW + 40);
        assert_eq!(check_export(&bytes[..], &backup)?, info);
        assert!(crate::shared_export::check_export(&bytes[..]).is_err());
        for destination in [512, 4096] {
            let (restored, image) = verified_image(&bytes[..], &backup)?;
            assert_eq!(info, restored);
            let mut p = SharedPool::format_p6(SimDevice::new(2048, destination)?, [31; 16])?;
            let clone = p.import_with_compression(
                OWNER,
                restored.ids,
                &image,
                restored.kind,
                Compression::Lz4,
            )?;
            drop(image);
            let mounted = SharedPool::open(p.into_device())?;
            assert_eq!(mounted.info(clone)?.kind, ObjectKind::Clone);
            assert_eq!(
                mounted.read_file(clone, "/dossier-prive", 4101, PRIVATE.len())?,
                PRIVATE
            );
        }
    }
    Ok(())
}

#[test]
fn refus_ferme_de_la_mauvaise_cle_des_troncatures_et_substitutions() -> Result<()> {
    let mut rng = HostEntropy;
    let backup = Secret::generate(&mut rng)?;
    let wrong = Secret::generate(&mut rng)?;
    for sector in [512, 4096] {
        let (p, clone) = source(sector)?;
        let mut original = Vec::new();
        let info = export_view(&p, clone, sector, DOMAIN, &backup, &mut rng, &mut original)?;
        assert!(info.pages >= 2);
        assert!(check_export(original.as_slice(), &wrong).is_err());
        let mut legacy = Vec::new();
        crate::shared_export::export_view(&p, clone, sector, &mut legacy)?;
        assert!(check_export(&legacy[..], &backup).is_err());
        for cut in [
            0,
            7,
            PREFIX - 1,
            PREFIX,
            103,
            START - 1,
            START + 8,
            START + 100,
            original.len() - 40,
            original.len() - 1,
        ] {
            assert!(
                check_export(&original[..cut], &backup).is_err(),
                "coupure {cut}"
            );
        }
        for at in [
            0,
            8,
            12,
            16,
            PREFIX,
            PREFIX + 24,
            PREFIX + 40,
            104,
            128,
            144,
            START,
            START + 8,
            START + 32,
            START + 48,
            original.len() - 40,
            original.len() - 1,
        ] {
            let mut altered = original.clone();
            altered[at] ^= 1;
            assert!(
                check_export(&altered[..], &backup).is_err(),
                "altération {at}"
            );
        }
        let mut altered = original.clone();
        altered.push(0);
        assert!(check_export(&altered[..], &backup).is_err());
        let mut second = Vec::new();
        export_view(&p, clone, sector, DOMAIN, &backup, &mut rng, &mut second)?;
        second[START..START + ROW].copy_from_slice(&original[START..START + ROW]);
        assert!(check_export(&second[..], &backup).is_err());
        for index in 0..ROW {
            original.swap(START + index, START + ROW + index);
        }
        assert!(check_export(&original[..], &backup).is_err());
    }
    Ok(())
}

struct Interrupted {
    remaining: usize,
    bytes: Vec<u8>,
}
impl Write for Interrupted {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(io::Error::other("panne simulée"));
        }
        let n = data.len().min(self.remaining);
        self.bytes.extend_from_slice(&data[..n]);
        self.remaining -= n;
        Ok(n)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn echec_de_la_production_laisse_un_flux_inutilisable() -> Result<()> {
    let mut rng = HostEntropy;
    let backup = Secret::generate(&mut rng)?;
    let (p, clone) = source(512)?;
    let mut sink = Interrupted {
        remaining: START + ROW + 500,
        bytes: Vec::new(),
    };
    assert!(export_view(&p, clone, 512, DOMAIN, &backup, &mut rng, &mut sink).is_err());
    assert!(check_export(&sink.bytes[..], &backup).is_err());
    Ok(())
}

#[test]
fn page_authentifiee_meme_avec_cloture_valide() -> Result<()> {
    let mut rng = HostEntropy;
    let backup = Secret::generate(&mut rng)?;
    let (p, clone) = source(4096)?;
    let mut original = Vec::new();
    let info = export_view(&p, clone, 4096, DOMAIN, &backup, &mut rng, &mut original)?;
    let id = original[16..32].try_into().unwrap();
    let wrap = backup.derive(&id, WRAP_KEY)?;
    let mut secret = Zeroizing::new([0; 32]);
    secret.copy_from_slice(&original[PREFIX + 40..PREFIX + 72]);
    wrap.open(
        original[PREFIX..PREFIX + 24].try_into().unwrap(),
        original[PREFIX + 24..PREFIX + 40].try_into().unwrap(),
        &original[..PREFIX],
        &mut secret[..],
    )?;
    let key = Secret::from_bytes(*secret).derive(&id, FOOT_KEY)?;
    for at in [START + 8, START + 32, START + 48] {
        let mut altered = original.clone();
        altered[at] ^= 1;
        let end = altered.len() - 40;
        let digest = Sha256::new().chain_update(&altered[..end]);
        altered[end + 8..].copy_from_slice(&key.mac(&[&footer(digest, info.pages)]));
        assert!(
            verified_image(&altered[..], &backup).is_err(),
            "champ de la page {at}"
        );
    }
    Ok(())
}

/// Les variables permettent de transporter le vecteur public vers le lecteur
/// d'une autre architecture sans écrire de clé ou de donnée en clair sur disque.
#[test]
fn vecteur_dechange_entre_architectures() -> Result<()> {
    let key = Secret::from_bytes([23; 32]); // Clé publique réservée à ce seul vecteur synthétique.
    for sector in [512, 4096] {
        let (p, clone) = source(sector)?;
        let mut bytes = Vec::new();
        export_view(
            &p,
            clone,
            sector,
            DOMAIN,
            &key,
            &mut HostEntropy,
            &mut bytes,
        )?;
        if let Some(dir) = std::env::var_os("NOVAFS_P7_EXPORT_SEND") {
            let mut out = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(std::path::Path::new(&dir).join(format!("archive-{sector}.nvx")))?;
            out.write_all(&bytes)?;
        }
        if let Some(dir) = std::env::var_os("NOVAFS_P7_EXPORT_RECEIVE") {
            let file = std::fs::File::open(
                std::path::Path::new(&dir).join(format!("archive-{sector}.nvx")),
            )?;
            let (info, image) = verified_image(file, &key)?;
            assert_eq!((info.sector, info.kind), (sector, ObjectKind::Clone));
            let mut p = SharedPool::format_p6(SimDevice::new(2048, sector)?, [121; 16])?;
            let clone =
                p.import_with_compression(OWNER, info.ids, &image, info.kind, Compression::Lz4)?;
            assert_eq!(
                p.read_file(clone, "/dossier-prive", 4101, PRIVATE.len())?,
                PRIVATE
            );
        }
        assert_eq!(check_export(&bytes[..], &key)?.sector, sector);
    }
    Ok(())
}
