//! Modèle hôte sur un unique SimDevice neuf. Les enregistrements suivent le LV
//! de la fixture et n'appartiennent à aucun format reconnu par le service.
use super::*;
use crate::{Block, Ref, codec::Anchor};
use sha2::{Digest, Sha256};

mod injections;

// Un point de départ et deux publications. Les exemplaires occupent deux
// groupes d'adresses disjoints, dans le même domaine de panne.
const PAIRES: u64 = 3;
const PAGE: usize = 4096;
const SOMME: usize = PAGE - 32;

fn adresses(taille_lv: u64, position: u64) -> [u64; 2] {
    [taille_lv + position, taille_lv + PAIRES + position]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Decision {
    parent: Ref,
    tete: Ref,
}

impl Decision {
    fn encoder(self, ids: Ids, taille: u64, adresse: u64) -> Block {
        let mut page = [0; PAGE];
        page[..8].copy_from_slice(b"NVLABTX1");
        page[8..24].copy_from_slice(&ids.fs);
        page[24..40].copy_from_slice(&ids.lv);
        page[40..48].copy_from_slice(&taille.to_le_bytes());
        page[48..56].copy_from_slice(&adresse.to_le_bytes());
        page[56..104].copy_from_slice(&self.parent.to_bytes());
        page[104..152].copy_from_slice(&self.tete.to_bytes());
        let somme = Sha256::new()
            .chain_update(b"NovaFS.lab.publication.v1\0")
            .chain_update(&page[..SOMME])
            .finalize();
        page[SOMME..].copy_from_slice(&somme);
        page
    }

    fn decoder(page: &Block, ids: Ids, taille: u64, adresse: u64) -> Result<Self> {
        let parent = Ref::from_bytes(&page[56..104])?;
        let tete = Ref::from_bytes(&page[104..152])?;
        parent.validate(taille, tete.birth)?;
        tete.validate(taille, tete.birth)?;
        let decision = Self { parent, tete };
        if parent.birth.checked_add(1) != Some(tete.birth)
            || &decision.encoder(ids, taille, adresse) != page
        {
            return Err(Error::Corrupt("décision de publication invalide"));
        }
        Ok(decision)
    }
}

/// Présente au vérificateur P9 les deux références lues dans le journal.
/// Aucun cache d'ancre n'est écrit, même sur le simulateur.
struct Vue<'a> {
    disque: &'a SimDevice,
    taille: u64,
    ids: Ids,
    decision: Decision,
}

impl Device for Vue<'_> {
    fn blocks(&self) -> u64 {
        self.taille
    }
    fn read(&self, bloc: u64) -> Result<Block> {
        match bloc {
            0 | 1 => {
                let manifest = if self.decision.tete.birth % 2 == bloc {
                    self.decision.tete
                } else {
                    self.decision.parent
                };
                Anchor {
                    ids: self.ids,
                    blocks: self.taille,
                    manifest,
                }
                .encode(bloc)
            }
            b if b < self.taille => self.disque.read(b),
            _ => Err(Error::Corrupt("lecture hors du volume")),
        }
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        Err(Error::ReadOnly)
    }
    fn flush(&mut self) -> Result<()> {
        Err(Error::ReadOnly)
    }
}

struct Relecture<'a> {
    fs: Engine<Vue<'a>>,
    prochaine: u64,
    copies_manquantes_ou_invalides: usize,
    ancres_invalides: usize,
}

/// Entrées du lecteur : le support et l'identité du volume. Ni trace de panne,
/// ni acquittement client, ni reçu attendu ne servent à choisir la génération.
fn recuperer(disque: &SimDevice, ids: Ids) -> Result<Relecture<'_>> {
    let taille = disque
        .blocks()
        .checked_sub(2 * PAIRES)
        .filter(|&n| n >= 3)
        .ok_or(Error::Corrupt("géométrie du banc"))?;
    let mut derniere: Option<Decision> = None;
    let mut degradees = 0;
    let mut prochaine = 0;
    for position in 0..PAIRES {
        let mut selection = None;
        let mut vides = 0;
        let mut valides = 0;
        for adresse in adresses(taille, position) {
            let page = disque.read(adresse)?;
            if page == [0; PAGE] {
                vides += 1;
            } else if let Ok(recue) = Decision::decoder(&page, ids, taille, adresse) {
                if selection.replace(recue).is_some_and(|autre| autre != recue) {
                    return Err(Error::Corrupt("copies de décision divergentes"));
                }
                valides += 1;
            }
        }
        if let Some(recue) = selection {
            if position != prochaine || derniere.is_some_and(|d| d.tete != recue.parent) {
                return Err(Error::Corrupt("rupture du journal"));
            }
            derniere = Some(recue);
            prochaine += 1;
            degradees += 2 - valides;
        } else if vides != 2 {
            return Err(Error::Corrupt("publication ambiguë"));
        }
    }
    let decision = derniere.ok_or(Error::Corrupt("journal absent"))?;
    let mut ancres_invalides = 0;
    for slot in [0, 1] {
        let ancre = Anchor::decode(&disque.read(slot)?, slot);
        if !ancre.is_ok_and(|a| a.ids == ids && a.blocks == taille) {
            ancres_invalides += 1;
        }
    }
    // Echec sur tout graphe incomplet, même si le précédent est lisible.
    let fs = Engine::mount_current(
        Vue {
            disque,
            taille,
            ids,
            decision,
        },
        ids,
        Mode::ReadOnly,
    )?;
    Ok(Relecture {
        fs,
        prochaine,
        copies_manquantes_ou_invalides: degradees,
        ancres_invalides,
    })
}

fn pages_preparees(controle: &SimControl, tete: Ref) -> Result<Vec<(u64, Block)>> {
    let finalise = controle.snapshot().device();
    let mut pages = Vec::new();
    let mut blocs = std::collections::BTreeSet::new();
    for evenement in controle
        .events()
        .into_iter()
        .take_while(|e| *e != Event::FlushSubmitted(1))
    {
        if let Event::WriteCompleted(b) = evenement {
            assert!(b >= 2 && blocs.insert(b));
            pages.push((b, finalise.read(b)?));
        }
    }
    assert!(blocs.contains(&tete.block));
    Ok(pages)
}

// Le parcours a vérifié que toute la queue est vierge. L'écrivain unique
// refuse une reprise dégradée et ne recycle jamais un emplacement occupé.
fn ajouter(
    disque: &mut SimDevice,
    decision: Decision,
    pages: &[(u64, Block)],
    inverser: bool,
) -> Result<()> {
    let relu = recuperer(disque, IDS)?;
    if relu.copies_manquantes_ou_invalides != 0 || relu.ancres_invalides != 0 {
        return Err(Error::Corrupt("journal dégradé"));
    }
    if relu.prochaine == PAIRES {
        return Err(Error::NoSpace);
    }
    if relu.fs.receipt() != decision.parent {
        return Err(Error::Corrupt("point de départ de publication"));
    }
    let taille = disque.blocks() - 2 * PAIRES;
    let mut copies = adresses(taille, relu.prochaine);
    // L'ordre inverse éprouve aussi un manifeste écrit avant ses dépendances.
    for i in 0..pages.len() {
        let index = if inverser { pages.len() - 1 - i } else { i };
        let (b, page) = &pages[index];
        disque.write(*b, page)?;
    }
    disque.flush()?;
    if inverser {
        copies.reverse();
    }
    for adresse in copies {
        disque.write(adresse, &decision.encoder(IDS, taille, adresse))?;
    }
    disque.flush()
}

struct Experience {
    cas: Cas,
    depart: SimSnapshot,
    taille: u64,
    checkpoint: Decision,
    decision: Decision,
    dependances: Vec<(u64, Block)>,
}

impl Experience {
    fn neuve(secteur: u32, copie: bool) -> Result<Self> {
        let cas = Cas::neuf(secteur, copie, copie)?;
        let (mut fs, controle) = cas.ouvrir()?;
        let tete = fs.receipt();
        let base = cas.image.device();
        let taille = base.blocks();
        let slot_parent = 1 - tete.birth % 2;
        let parent = Anchor::decode(&base.read(slot_parent)?, slot_parent)?.manifest;
        let checkpoint = Decision { parent, tete };
        controle.trace(None, None);
        remplacer(&mut fs, cas.source, cas.destination)?;
        cas.verifier(&fs, true)?;
        let decision = Decision {
            parent: tete,
            tete: fs.receipt(),
        };
        let dependances = pages_preparees(&controle, decision.tete)?;
        let mut octets = cas.image.bytes()?;
        octets.resize(octets.len() + 2 * PAIRES as usize * PAGE, 0);
        let mut disque = SimDevice::from_bytes(&octets, secteur)?;
        for adresse in adresses(taille, 0) {
            disque.write(adresse, &checkpoint.encoder(IDS, taille, adresse))?;
        }
        disque.flush()?;
        let depart = disque.control().snapshot();
        Ok(Self {
            cas,
            depart,
            taille,
            checkpoint,
            decision,
            dependances,
        })
    }

    fn publier(&self, disque: &mut SimDevice, inverser: bool) -> Result<()> {
        ajouter(disque, self.decision, &self.dependances, inverser)
    }
}
