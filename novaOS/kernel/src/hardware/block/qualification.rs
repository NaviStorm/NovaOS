//! Profil de qualification mono-vCPU, virt-10.0. Aucun bail bloc n'est publié à EL0.
use super::{
    Error, Result, Transport,
    io::{PAGE, Region},
    pci::Config,
};
use crate::{arch::serial, hal};
use alloc::{format, vec, vec::Vec};

const SOURCE: &[u8] = b"NovaOS-Install-v1";
const CIBLE: &[u8] = b"NovaOS-Target-v1";
const SERIE: &str = "NOVA-CIBLE-000000001";
const MIN_OCTETS: u64 = 64 << 20;

#[derive(Clone, Copy, Eq, PartialEq)]
struct Roles {
    source: u8,
    cible: Option<u8>,
}

fn fichier_fw(nom: &[u8]) -> Result<Vec<u8>> {
    // Port PIO de fw_cfg sur virt. Sélecteur big-endian, flux d'octets sans DMA.
    let io = Region::map(0x0902_0000, 300, PAGE)?;
    io.w16(8, 0x19u16.to_be()); // FW_CFG_FILE_DIR
    let nombre = u32::from_be_bytes(core::array::from_fn(|_| io.r8(0)));
    if nombre > 256 {
        return Err(Error::Geometry);
    }
    let mut entree = None;
    for _ in 0..nombre {
        let e: [u8; 64] = core::array::from_fn(|_| io.r8(0));
        let fin = e[8..].iter().position(|&x| x == 0).ok_or(Error::Geometry)? + 8;
        if &e[8..fin] == nom {
            let taille = u32::from_be_bytes(e[..4].try_into().unwrap()) as usize;
            let select = u16::from_be_bytes([e[4], e[5]]);
            if entree.is_some() || !(6..=65536).contains(&taille) || select < 0x20 {
                return Err(Error::Geometry);
            }
            entree = Some((select, taille));
        }
    }
    let (select, taille) = entree.ok_or(Error::Absent)?;
    let mut table = vec![0; taille];
    io.w16(8, select.to_be());
    for b in &mut table {
        *b = io.r8(0);
    }
    Ok(table)
}

fn profil(systeme: bool) -> Result<Roles> {
    let table = fichier_fw(b"etc/smbios/smbios-tables")?;
    let mut suite = table.as_slice();
    let (mut source, mut cible) = (None, None);
    loop {
        let entete = suite.get(..4).ok_or(Error::Geometry)?;
        let taille = usize::from(entete[1]);
        if taille < 4 || taille > suite.len() {
            return Err(Error::Geometry);
        }
        let (champs, chaines) = suite.split_at(taille);
        let fin = chaines
            .windows(2)
            .position(|x| x == [0, 0])
            .ok_or(Error::Geometry)?;
        if entete[0] == 41 {
            if champs.len() != 11 || champs[4] != 1 {
                return Err(Error::Geometry);
            }
            let nom = chaines[..fin].split(|&x| x == 0).next().unwrap_or_default();
            let role = match nom {
                SOURCE if !systeme => Some(&mut source),
                b"NovaOS-System-v1" if systeme => Some(&mut source),
                SOURCE | b"NovaOS-System-v1" => return Err(Error::Geometry),
                CIBLE => Some(&mut cible),
                _ => None,
            };
            if let Some(place) = role {
                if champs[5..10] != [0x81, 1, 0, 0, 0]
                    || champs[10] & 7 != 0
                    || place.replace(champs[10]).is_some()
                {
                    return Err(Error::Geometry);
                }
            }
        }
        suite = &chaines[fin + 2..];
        if entete[0] == 127 {
            if !suite.is_empty() {
                return Err(Error::Geometry);
            }
            break;
        }
    }
    let source = source.ok_or(Error::Absent)?;
    if cible == Some(source) || (systeme && cible.is_some()) {
        return Err(Error::Geometry);
    }
    Ok(Roles { source, cible })
}

fn identite(t: &mut Transport) -> Result<()> {
    t.p.0[20].mem.put(0, &[0xff; 20]);
    t.begin_input(0, 8, 20)?; // VIRTIO_BLK_T_GET_ID
    t.finish(21)?;
    let mut id = [0; 20];
    t.p.0[20].mem.bytes(0, &mut id);
    if id != SERIE.as_bytes() {
        return Err(Error::Device);
    }
    Ok(())
}

fn verifier_zero(t: &mut Transport) -> Result<()> {
    let mut lba = 0;
    let data = t.p.0[20].mem;
    while lba < t.sectors {
        let blocs = (t.sectors - lba).min((PAGE / t.size) as u64);
        let octets = blocs as usize * t.size;
        // Une complétion qui laisserait le tampon intact ne vaut jamais preuve de zéros.
        for i in (0..octets).step_by(8) {
            data.w64(i, u64::MAX);
        }
        t.begin_input(lba, 0, octets)?;
        t.finish(1 + octets as u32)?;
        if let Some(i) = (0..octets).step_by(8).find(|&i| data.r64(i) != 0) {
            let premier = (i..i + 8).find(|&n| data.r8(n) != 0).ok_or(Error::Fault)?;
            hal::log(format_args!(
                "CIBLE non vierge octet={}",
                lba * t.size as u64 + premier as u64
            ));
            return Err(Error::Device);
        }
        lba += blocs;
    }
    if t.d.geometry()? != (t.sectors, t.size) {
        return Err(Error::Geometry);
    }
    t.unit.faults(&t.p)
}

fn confirmer(capacite: u64) -> bool {
    while serial::input().is_some() {}
    let attendu = format!("AUTORISER {SERIE} {capacite}");
    hal::log(format_args!(
        "CIBLE confirmation requise, saisir exactement : {attendu}"
    ));
    let mut position = 0;
    let mut valide = true;
    loop {
        if let Some(c) = serial::input() {
            if matches!(c, b'\r' | b'\n') {
                let _ = serial::output("\r\n");
                return valide && position == attendu.len();
            }
            if c.is_ascii_graphic() || c == b' ' {
                let lettre = [c];
                let _ = serial::output(core::str::from_utf8(&lettre).unwrap());
            }
            valide &= attendu.as_bytes().get(position) == Some(&c);
            position = position.saturating_add(1);
        } else {
            core::hint::spin_loop();
        }
    }
}

fn inspecter(t: &mut Transport, profil: Roles, sid: u8) -> Result<bool> {
    identite(t)?;
    let capacite = t
        .sectors
        .checked_mul(t.size as u64)
        .ok_or(Error::Geometry)?;
    hal::log(format_args!(
        "CIBLE serie={SERIE} sid={sid} capacite={capacite} secteur={}",
        t.size
    ));
    if t.ro || capacite < MIN_OCTETS {
        return Err(Error::Geometry);
    }
    verifier_zero(t)?;
    hal::log(format_args!("CIBLE vierge octets={capacite}"));
    if !confirmer(capacite) {
        return Ok(false);
    }
    if self::profil(false)? != profil || t.d.geometry()? != (t.sectors, t.size) || t.d.read_only() {
        return Err(Error::Geometry);
    }
    identite(t)?;
    verifier_zero(t)?;
    Ok(true)
}

fn selection() -> Result<(Roles, u8)> {
    let profil = profil(false)?;
    let source = Config::open(profil.source)?;
    if source.get(0, 4) != 0x1042_1af4 || source.get(4, 2) & 4 != 0 {
        hal::log(format_args!(
            "CIBLE transfert source refuse SID={} commande={:#x}",
            profil.source,
            source.get(4, 2)
        ));
        return Err(Error::Quarantine);
    }
    let Some(sid) = profil.cible else {
        hal::log(format_args!("CIBLE absente"));
        return Err(Error::Absent);
    };
    Ok((profil, sid))
}

fn autoriser_sortie(t: &mut Transport) -> Result<()> {
    if t.unit.map(&t.p, 6, t.p.0[22].pa, 1).is_err() {
        t.d.cfg.master(false);
        return Err(Error::Quarantine);
    }
    t.output_mapped = true;
    Ok(())
}

pub(crate) fn executer() -> Result<()> {
    let (profil, sid) = selection()?;
    let mut t = Transport::open_disk(sid, false)?;
    let decision = inspecter(&mut t, profil, sid);
    if let Ok(true) = decision {
        // En cas d'acquittement incertain, les cadres restent épinglés jusqu'au reset de la VM.
        autoriser_sortie(&mut t)?;
        hal::log(format_args!("CIBLE droit WRITE accorde serie={SERIE}"));
    } else if let Ok(false) = decision {
        hal::log(format_args!("CIBLE confirmation refusee"));
    }
    // La qualification s'arrête ici. Aucune commande OUT, aucun FLUSH, aucun formatage.
    t.close()?;
    decision.map(|_| ())
}

// Le lanceur du noyau transfère le transport déjà contrôlé, sans nouvelle découverte PCI.
#[cfg(feature = "systeme-gpt")]
fn publier(mut t: Transport, table: super::gpt::Table, mode: u64) -> Result<()> {
    t.first = table.volume;
    t.count = table.end - table.volume;
    let key = super::lease::begin_with(15, || Ok(t))?;
    if !crate::sched::process::storage::volume_avec(mode, Some((key, table.volume_id()))) {
        return Err(Error::Device);
    }
    Ok(())
}

#[cfg(feature = "systeme-gpt")]
pub(crate) fn systeme() -> Result<()> {
    let sid = profil(true)?.source;
    let mut t = Transport::open_disk(sid, false)?;
    let verified = (|| {
        identite(&mut t)?;
        if t.ro {
            return Err(Error::Geometry);
        }
        super::gpt::Table::open(&mut t)
    })();
    let table = match verified {
        Ok(v) => v,
        Err(e) => {
            t.close()?;
            return Err(e);
        }
    };
    autoriser_sortie(&mut t)?;
    publier(t, table, 41)
}

#[cfg(feature = "installation-gpt")]
fn esp(t: &mut Transport, table: &super::gpt::Table, write: bool) -> Result<()> {
    let paquet: &[u8] = if t.size == 512 {
        include_bytes!(concat!(env!("NOVA_ESP"), "/esp-512.bin"))
    } else {
        include_bytes!(concat!(env!("NOVA_ESP"), "/esp-4096.bin"))
    };
    let stride = t.size + 4;
    if paquet.is_empty() || paquet.len() % stride != 0 {
        return Err(Error::Geometry);
    }
    let index = |r: &[u8]| u64::from(u32::from_le_bytes(r[..4].try_into().unwrap()));
    let mut last = None;
    for r in paquet.chunks_exact(stride) {
        let lba = index(r);
        if last.is_some_and(|v| lba <= v) || lba >= table.volume - table.esp {
            return Err(Error::Geometry);
        }
        last = Some(lba);
    }
    if write {
        for r in paquet.chunks_exact(stride) {
            t.p.0[22].mem.put(0, &r[4..]);
            t.output(table.esp + index(r), false)?;
        }
        return t.output(0, true);
    }
    let mut buf = vec![0; PAGE];
    let mut records = paquet.chunks_exact(stride).peekable();
    for start in (table.esp..table.volume).step_by(PAGE / t.size) {
        t.begin_input(start, 0, PAGE)?;
        t.finish(1 + PAGE as u32)?;
        t.p.0[20].mem.bytes(0, &mut buf);
        for (i, bytes) in buf.chunks_exact(t.size).enumerate() {
            let lba = start + i as u64 - table.esp;
            if records.peek().is_some_and(|r| index(r) == lba) {
                if bytes != &records.next().unwrap()[4..] {
                    return Err(Error::Device);
                }
            } else if bytes.iter().any(|&b| b != 0) {
                return Err(Error::Device);
            }
        }
    }
    Ok(())
}

#[cfg(feature = "installation-gpt")]
pub(crate) fn installer() -> Result<()> {
    let (roles, sid) = selection()?;
    // Trois EFI_GUID v4 frais, préparés pour cette installation dans le profil hôte.
    let input = fichier_fw(b"opt/novaos/gpt-guids")?;
    if input.len() != 48 {
        return Err(Error::Geometry);
    }
    let ids: [[u8; 16]; 3] =
        core::array::from_fn(|i| input[16 * i..16 * i + 16].try_into().unwrap());
    if ids.iter().any(|id| id[7] >> 4 != 4 || id[8] >> 6 != 2) {
        return Err(Error::Geometry);
    }
    let mut t = Transport::open_disk(sid, false)?;
    let preparer = (|| {
        let table = super::gpt::Table::new(&t, ids)?;
        if !inspecter(&mut t, roles, sid)? {
            return Err(Error::State);
        }
        autoriser_sortie(&mut t)?;
        hal::log(format_args!("INSTALLATION ecriture autorisee SID={sid}"));
        table.transfer(&mut t, true)?;
        table.transfer(&mut t, false)?;
        hal::log(format_args!(
            "INSTALLATION GPT verifiee ESP={} P9={} fin={}",
            table.esp, table.volume, table.end
        ));
        esp(&mut t, &table, true)?;
        esp(&mut t, &table, false)?;
        hal::log(format_args!("INSTALLATION ESP relue"));
        Ok(table)
    })();
    let table = match preparer {
        Ok(v) => v,
        Err(Error::Quarantine) => return Err(Error::Quarantine),
        Err(e) => {
            t.close()?;
            return Err(e);
        }
    };
    publier(t, table, 43)
}
