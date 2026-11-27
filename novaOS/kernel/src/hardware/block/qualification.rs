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

#[cfg(feature = "ancres-p9-lab")]
pub(crate) mod ancres;

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

fn premier_non_nul(t: &mut Transport) -> Result<Option<u64>> {
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
            return Ok(Some(lba * t.size as u64 + premier as u64));
        }
        lba += blocs;
    }
    if t.d.geometry()? != (t.sectors, t.size) {
        return Err(Error::Geometry);
    }
    t.unit.faults(&t.p)?;
    Ok(None)
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

enum Admission {
    Confirmee,
    Annulee,
    Occupe(u64),
}

fn inspecter(t: &mut Transport, profil: Roles, sid: u8) -> Result<Admission> {
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
    if let Some(offset) = premier_non_nul(t)? {
        return Ok(Admission::Occupe(offset));
    }
    hal::log(format_args!("CIBLE vierge octets={capacite}"));
    hal::log(format_args!("DISQUE vierge verifie"));
    if !confirmer(capacite) {
        return Ok(Admission::Annulee);
    }
    if self::profil(false)? != profil || t.d.geometry()? != (t.sectors, t.size) || t.d.read_only() {
        return Err(Error::Geometry);
    }
    identite(t)?;
    if let Some(offset) = premier_non_nul(t)? {
        return Ok(Admission::Occupe(offset));
    }
    Ok(Admission::Confirmee)
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
    t.traductions = 7; // enregistrer le cadre avant la publication, même en cas de timeout
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
    if let Ok(Admission::Confirmee) = decision {
        // En cas d'acquittement incertain, les cadres restent épinglés jusqu'au reset de la VM.
        autoriser_sortie(&mut t)?;
        hal::log(format_args!("CIBLE droit WRITE accorde serie={SERIE}"));
    } else if let Ok(Admission::Annulee) = decision {
        hal::log(format_args!("CIBLE confirmation refusee"));
    }
    // La qualification s'arrête ici. Aucune commande OUT, aucun FLUSH, aucun formatage.
    t.close()?;
    match decision? {
        Admission::Occupe(offset) => {
            hal::log(format_args!("CIBLE refusee octet={offset}"));
            Err(Error::Device)
        }
        _ => Ok(()),
    }
}

// Le lanceur du noyau transfère le transport déjà contrôlé, sans nouvelle découverte PCI.
#[cfg(feature = "systeme-gpt")]
fn publier(mut t: super::Bail, table: super::gpt::Table, mode: u64) -> Result<()> {
    t.first = table.volume;
    t.count = table.end - table.volume;
    let rights = if mode == 44 { 1 | 8 } else { 15 };
    let mut owned = Some(t);
    let key = match super::lease::begin_with(rights, || owned.take().ok_or(Error::State)) {
        Ok(key) => key,
        Err(error) => {
            if let Some(disk) = owned {
                disk.close()?;
            }
            return Err(error);
        }
    };
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
    // Le profil graphique transporte les mêmes octets en hexadécimal dans fw_cfg.
    let mut raw = [0u8; 48];
    match input.len() {
        48 => raw.copy_from_slice(&input),
        96 => {
            for (out, pair) in raw.iter_mut().zip(input.chunks_exact(2)) {
                let hex = core::str::from_utf8(pair).map_err(|_| Error::Geometry)?;
                if !pair.iter().all(u8::is_ascii_hexdigit) {
                    return Err(Error::Geometry);
                }
                *out = u8::from_str_radix(hex, 16).map_err(|_| Error::Geometry)?;
            }
        }
        _ => return Err(Error::Geometry),
    }
    let ids: [[u8; 16]; 3] = core::array::from_fn(|i| raw[i * 16..i * 16 + 16].try_into().unwrap());
    if ids.iter().any(|id| id[7] >> 4 != 4 || id[8] >> 6 != 2) {
        return Err(Error::Geometry);
    }
    let mut t = Transport::open_disk(sid, false)?;
    let table = match super::gpt::Table::new(&t, ids) {
        Ok(table) => table,
        Err(e) => {
            hal::log(format_args!("DISQUE indetermine geometrie"));
            t.close()?;
            return Err(e);
        }
    };
    match inspecter(&mut t, roles, sid) {
        Ok(Admission::Confirmee) => (),
        Ok(Admission::Annulee) => {
            t.close()?;
            return Err(Error::State);
        }
        Ok(Admission::Occupe(offset)) => {
            use super::gpt::Etat;
            match table.classer(&mut t, offset) {
                Ok(Etat::Volume(volume)) => {
                    // L'ESP doit contenir le système attendu avant d'en attribuer l'origine.
                    if let Err(e) = esp(&mut t, &volume, false) {
                        hal::log(format_args!("DISQUE indetermine verification ESP"));
                        t.close()?;
                        return Err(e);
                    }
                    // Aucun tampon OUT. Le service vérifie P9 sur un bail READ/CLOSE borné.
                    publier(t, volume, 44)?;
                }
                classification => {
                    let message = match classification {
                        Ok(Etat::Interrompu) => "installation interrompue",
                        Ok(Etat::Incoherent) => "metadonnees incoherentes",
                        Ok(Etat::Occupe) => "non vierge origine indeterminee",
                        Err(_) => "indetermine lecture",
                        Ok(Etat::Volume(_)) => unreachable!(),
                    };
                    hal::log(format_args!("DISQUE {message}"));
                    t.close()?;
                }
            }
            return Err(Error::Device);
        }
        Err(e) => {
            hal::log(format_args!("DISQUE indetermine identification ou lecture"));
            t.close()?;
            return Err(e);
        }
    }
    let preparer = (|| {
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

/// Recette privée sur le seul support système marqué de la fixture. La fenêtre
/// inscriptible reste celle de la partition P9 vérifiée par les deux GPT.
#[cfg(feature = "net-arm-mesure")]
pub(crate) fn net_p9() -> Result<()> {
    if profil(true)?.source != 48 || crate::smp::count() != 1 {
        return Err(Error::Route);
    }
    let ouvrir = || -> Result<(super::Bail, super::gpt::Table)> {
        let mut t = Transport::open_disk(48, false)?;
        identite(&mut t)?;
        let table = super::gpt::Table::open(&mut t)?;
        autoriser_sortie(&mut t)?;
        Ok((t, table))
    };
    #[cfg(all(feature="net-ethernet-lab",feature="net-arm-lab"))]
    crate::sched::process::net_preparation::mesurer("net_initial");
    let (mut t, table) = ouvrir()?;
    #[cfg(feature = "net-arm-lab")]
    #[allow(unused_mut)] // Le précontrôle 3E réouvre le disque. Le profil 4D le garde actif.
    let mut table = table;
    let mode = match fichier_fw(b"opt/novaos/net-p9") {
        Err(Error::Absent) => 25,
        Ok(v) => match v.strip_suffix(&[0]).unwrap_or(&v) {
            b"avant-f1" => 14,
            b"apres-f1" => 16,
            b"avant-f2" => 17,
            b"apres-f2" => 19,
            b"files-nic" => 125,
            #[cfg(all(feature="net-arm-lab",feature="net-ethernet-lab"))]
            b"ethernet-prive" => 126,
            #[cfg(all(feature="net-arm-lab",feature="net-ethernet-lab"))]
            b"ethernet-p9" => 127,
            b"parcours-fichiers" => 11,
            b"remonter-ro" => 13,
            _ => return Err(Error::Geometry),
        },
        Err(e) => return Err(e),
    };
    let secteur = t.size;
    #[cfg(feature = "net-arm-lab")]
    let nic = {
        #[cfg(feature="gestionnaire-4d")]
        let mut premier_cycle=Some(crate::sched::process::net_preparation::dynamique::Cycle::ouvrir());
        let nic = match super::net_arm::Preparation::ouvrir() {
            Ok(nic) => nic,
            Err(e) => {
                #[cfg(feature="gestionnaire-4d")]
                {t.begin_read(table.volume)?;t.finish_read()?;t.output(0,true)?;hal::log(format_args!("D4 refus stockage lecture_flush=true"));}
                t.close()?;
                hal::log(format_args!("NET ARM refus preparation={e:?} cadres_dma={}", crate::memory::pinned_dma_frames()));
                return Err(e);
            }
        };
        let lba = table.volume;
        #[cfg(feature="gestionnaire-4d")]
        let suite = nic;
        #[cfg(not(feature="gestionnaire-4d"))]
        let suite = {
        let pico = crate::memory::pinned_dma_frames();
        let (controle, endpoint, vol, gardien) = super::domain::Domain::dimensions();
        hal::log(format_args!(
            "NET ARM tailles controle={controle} endpoint={endpoint} vol={vol} statique_controle={gardien} preparation={}",
            core::mem::size_of_val(&nic)
        ));
        let generation = nic.domaine.generation();
        if !crate::sched::process::net_preparation::tester(generation, || {
            nic.preparer_files(mode == 125).is_ok()
        }) {
            nic.fermer()?;
            return Err(Error::Route);
        }
        // L'IPC protège la validation et les écritures MMIO sans blocage.
        // La file IOMMU a son propre mutex, consulté après libération de cette garde.
        if let Err(e) = nic.domaine.faults_preparation() {
            nic.fermer()?;
            return Err(e);
        }
        if let Err(e) = nic.verifier_route_interruptions() {
            nic.fermer()?;
            return Err(e);
        }
        t.begin_read(lba)?;
        let seq = nic.domaine.differer_nic()?;
        t.unit.verifier_pression(seq)?;
        t.finish_read()?;
        if nic.domaine.reconcilier_nic(seq - 1) != Err(Error::State) {
            return Err(Error::Fault);
        }
        nic.domaine.reconcilier_nic(seq)?;
        nic.domaine.reconcilier_nic(seq)?;
        hal::log(format_args!("NET ARM UNMAP NIC seq={seq} acquitte et relu, autre numero refuse"));
        nic.retablir_premiere_page()?;
        hal::log(format_args!(
            "NET ARM attente NIC seq={seq} disque=contre-pression lecture_admise=achevee pic_dma={pico}"
        ));
        // Le disque ferme intégralement son domaine pendant que le NIC conserve le sien.
        t.close()?;
        (t, table) = ouvrir()?;
        hal::log(format_args!(
            "NET ARM disque retire puis rattache NIC conserve generation={generation}"
        ));
        let before = crate::memory::pinned_dma_frames();
        nic.fermer()?;
        t.begin_read(table.volume)?;
        t.finish_read()?;
        if before.checked_sub(crate::memory::pinned_dma_frames()) != Some(22) {
            return Err(Error::Quarantine);
        }
        let suite = super::net_arm::Preparation::ouvrir()?;
        if suite.domaine.generation() <= generation {
            return Err(Error::State);
        }
        hal::log(format_args!(
            "NET ARM NIC retire disque lisible pages_rendues=22 nouvelle_generation={}",
            suite.domaine.generation()
        ));
        suite
        };
        #[cfg(feature="net-ethernet-lab")]
        if matches!(mode,126|127) {
            #[cfg(feature="ethernet-v1-port")]
            let cycles = match option_env!("NOVAOS_ETH3_CASE") {
                Some("cycles-256") => 256,
                Some("cycles" | "cycles-court") => 4,
                Some("rattrapage" | "recu-arm-differe" | "reset-court" | "reset-iommu-differe") => 2,
                _ => 1,
            };
            #[cfg(not(feature="ethernet-v1-port"))]
            let cycles = 1;
            let mut premier = Some(suite);
            for cycle in 0..cycles {
            #[cfg(feature="gestionnaire-4d")]
            let _mesure=premier_cycle.take().unwrap_or_else(crate::sched::process::net_preparation::dynamique::Cycle::ouvrir);
            let nic = match premier.take() {
                Some(nic) => nic,
                None => super::net_arm::Preparation::ouvrir()?,
            };
            let generation = nic.domaine.generation();
            let trafic=nic.trames(||{
                if mode==126 {return true}
                let mut reads=[0u64;64];
                let mut flushes=[0u64;16];
                let hz=hal::timer::counter_hz();
                let mut bloc=[0u8;PAGE];
                for (i,mesure) in reads.iter_mut().enumerate(){
                    let debut=hal::timer::counter();
                    if t.begin_input(table.volume + (i*(PAGE/secteur)) as u64, 0, PAGE).is_err() || t.finish(4097).is_err() {return false}
                    *mesure=hal::timer::counter()-debut;
                    if i==0 {
                        t.p.0[20].mem.read_page(&mut bloc);
                        if &bloc[..8]!=b"NOVAP0FS" {return false}
                    }
                }
                for mesure in &mut flushes{
                    let debut=hal::timer::counter();
                    if t.output(0,true).is_err(){return false}
                    *mesure=hal::timer::counter()-debut;
                }
                hal::log(format_args!("NET ARM lectures secteur={secteur} hz={hz} octets=4096 cycles={reads:?}"));
                hal::log(format_args!("NET ARM flush hz={hz} cycles={flushes:?}"));
                true
            });
            #[cfg(feature="gestionnaire-4d")]
            if matches!(option_env!("NOVAOS_4D_REFUS"),Some("lancement"|"attribution"|"deces-gestionnaire")) && trafic==Err(Error::State) {
                t.begin_read(lba)?;t.finish_read()?;t.output(0,true)?;t.close()?;
                hal::log(format_args!("D4 echec scelle stockage lecture_flush=true cadres_dma={}",crate::memory::pinned_dma_frames()));
                return Err(Error::State)
            }
            #[cfg(feature="ethernet-v1-port")]
            let trafic = if trafic.is_err()
                && let Some((numero, epingles)) = super::net_arm::Preparation::attente_retrait(generation)
            {
                use super::net_arm::Preparation;
                // Le stockage conserve son domaine et son bail pendant la réconciliation.
                t.begin_read(table.volume)?;
                t.finish_read()?;
                t.output(0, true)?;
                if !matches!(Preparation::ouvrir(), Err(Error::Quarantine))
                    || !matches!(Preparation::reprendre_retrait(generation-1, numero), Err(Error::State))
                    || !matches!(Preparation::reprendre_retrait(generation, numero-1), Err(Error::State)) {
                    return Err(Error::Fault);
                }
                hal::log(format_args!("E3C retrait differe VirtIO-IOMMU generation={generation} numero={numero} anciennes_references_refusees=true epingles_nic={epingles} lecture_flush=true"));
                if matches!(option_env!("NOVAOS_ETH3_CASE"), Some("recu-arm-absent" | "reset-iommu-absent")) {
                    Err(Error::Quarantine)
                } else {
                    let resultat = Preparation::reprendre_retrait(generation, numero);
                    if resultat.is_ok() && !matches!(Preparation::reprendre_retrait(generation, numero), Err(Error::State)) {
                        return Err(Error::Fault);
                    }
                    resultat
                }
            } else { trafic };
            #[cfg(feature="ethernet-v1-port")]
            if let Err(e)=trafic {
                t.begin_read(table.volume)?;
                t.finish_read()?;
                if !matches!(super::net_arm::Preparation::ouvrir(), Err(Error::Quarantine)) {
                    return Err(Error::Fault);
                }
                hal::log(format_args!("E3C rattachement refuse generation={generation} disque_lisible=true"));
                t.close()?;
                hal::log(format_args!("ETH3 quarantaine disque_lisible=true cadres_retenus={}",crate::memory::pinned_dma_frames()));
                return Err(e);
            }
            trafic?;
            t.begin_read(table.volume)?;
            t.finish_read()?;
            #[cfg(feature="ethernet-v1-port")]
            hal::log(format_args!("E3C cycle={} generation={generation} disque_lisible=true dma_residents={}",cycle+1,crate::memory::pinned_dma_frames()));
            }
            t.close()?;
            let fin=crate::memory::pinned_dma_frames();
            hal::log(format_args!("NET ARM L2 retrait disque_lisible=true cadres_fin={fin}"));
            crate::sched::process::net_preparation::mesurer("net_final");
            if fin!=0 {return Err(Error::Quarantine)}
            return Ok(())
        }
        suite
    };
    let hz = hal::timer::counter_hz();
    let mut samples = [0u64; 64];
    let mut buf = [0; PAGE];
    let n = PAGE / secteur;
    for (i, sample) in samples.iter_mut().enumerate() {
        let debut = hal::timer::counter();
        t.begin_input(table.volume + (i * n) as u64, 0, PAGE)?;
        t.finish(4097)?;
        t.p.0[20].mem.read_page(&mut buf);
        if i == 0 && &buf[..8] != b"NOVAP0FS" {
            return Err(Error::Device);
        }
        *sample = hal::timer::counter() - debut;
    }
    hal::log(format_args!(
        "NET ARM lectures secteur={secteur} hz={hz} octets=4096 cycles={samples:?}"
    ));
    let mut flushes = [0u64; 16];
    for sample in &mut flushes {
        let debut = hal::timer::counter();
        t.output(0, true)?;
        *sample = hal::timer::counter() - debut;
    }
    hal::log(format_args!("NET ARM flush hz={hz} cycles={flushes:?}"));
    t.first = table.volume;
    t.count = table.end - table.volume;
    let droits = if mode == 13 {
        t.ro = true;
        1 | 8
    } else {
        15
    };
    let key = super::lease::begin_with(droits, || Ok(t))?;
    let debut = hal::timer::counter();
    let ok = crate::sched::process::storage::volume_detail(
        if mode == 125 { 25 } else { mode },
        Some((key, table.volume_id())),
        true,
    );
    let cycles = hal::timer::counter() - debut;
    hal::log(format_args!("NET ARM service_p9 cycles={cycles} hz={hz}"));
    #[cfg(feature = "net-arm-lab")]
    {
        nic.fermer()?;
        let fin = crate::memory::pinned_dma_frames();
        hal::log(format_args!("NET ARM terminaison cadres_dma={fin}"));
        if fin != 0 {
            return Err(Error::Quarantine);
        }
    }
    if !ok {
        return Err(Error::Device);
    }
    Ok(())
}
