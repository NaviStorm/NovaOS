//! Sonde AArch64 privée du laboratoire. Le plan fixe vient de fw_cfg et ne
//! désigne que des secteurs de la fenêtre d'essai sur un disque marqué jetable.
use super::*;

#[cfg(feature = "pc1-lab")]
mod pc1;

const MARQUE: &[u8; 8] = b"NVP9LAB1";
const FLUSH: u32 = u32::MAX;
const MIO: u64 = 1 << 20;

fn nombre(s: &[u8]) -> u32 {
    u32::from_le_bytes(s.try_into().unwrap())
}

fn attendre(octet: u8) {
    while serial::input() != Some(octet) {
        core::hint::spin_loop();
    }
}

fn plan<'a>(t: &mut Transport, entree: &'a [u8]) -> Result<(usize, Vec<(u32, &'a [u8])>)> {
    identite(t)?;
    let (debut, capacite) = match t.size {
        512 => (61 * MIO, 64 * MIO),
        4096 => (265 * MIO, 268 * MIO),
        _ => return Err(Error::Geometry),
    };
    if t.ro
        || t.sectors != capacite / t.size as u64
        || entree.len() < 32
        || &entree[..8] != MARQUE
        || nombre(&entree[8..12]) as usize != t.size
    {
        return Err(Error::Geometry);
    }
    let mut etiquette = vec![0; t.size];
    t.read(0, &mut etiquette)?;
    // Identifiant aléatoire de cette cible, indépendant de toute génération P9.
    if etiquette[..28] != entree[..28] || etiquette[28..].iter().any(|&v| v != 0) {
        return Err(Error::Device);
    }
    let pause = nombre(&entree[28..32]) as usize;
    let mut octets = &entree[32..];
    let mut ops = Vec::new();
    while !octets.is_empty() {
        let (adr, suite) = octets.split_at_checked(4).ok_or(Error::Geometry)?;
        let lba = nombre(adr);
        let taille = if lba == FLUSH { 0 } else { t.size };
        let (contenu, reste) = suite.split_at_checked(taille).ok_or(Error::Geometry)?;
        if lba != FLUSH {
            let pos = u64::from(lba) * t.size as u64;
            if pos < debut || pos + t.size as u64 > debut + 2 * MIO + t.size as u64 {
                return Err(Error::Geometry);
            }
        }
        ops.push((lba, contenu));
        octets = reste;
    }
    if ops.is_empty() || pause > ops.len() || ops.last().map(|op| op.0) != Some(FLUSH) {
        return Err(Error::Geometry);
    }
    Ok((pause, ops))
}

pub(crate) fn executer() -> Result<()> {
    let (roles, sid) = selection()?;
    if roles.source != 48 || sid != 56 {
        return Err(Error::Geometry);
    }
    let entree = fichier_fw(b"opt/novaos/ancres-p9-lab")?;
    let mut t = Transport::open_disk(sid, false)?;
    #[cfg(feature = "pc1-lab")]
    if entree.starts_with(b"PC1:") {
        let resultat = pc1::essai(&mut t, &entree[4..]);
        t.close()?;
        return resultat;
    }
    let (pause, ops) = match plan(&mut t, &entree) {
        Ok(plan) => plan,
        Err(e) => {
            t.close()?;
            return Err(e);
        }
    };
    autoriser_sortie(&mut t)?;
    hal::log(format_args!("P9LAB pret secteur={}", t.size));
    attendre(b'S');
    for i in 0..=ops.len() {
        if i == pause {
            hal::log(format_args!("P9LAB pause {i}/{}", ops.len()));
            attendre(b'G');
        }
        if let Some(&(lba, donnees)) = ops.get(i) {
            if lba == FLUSH {
                t.output(0, true)?;
            } else {
                t.p.0[22].mem.put(0, donnees);
                t.output(u64::from(lba), false)?;
            }
        }
    }
    hal::log(format_args!("P9LAB succes apres F2"));
    attendre(b'C');
    t.close()
}
