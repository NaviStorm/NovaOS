//! Banc du remplacement de deux entrées P9 déjà identifiées, sur volumes jetables.
use super::*;
use crate::device::{Event, Persistence, SimControl, SimDevice, SimSnapshot};
use crate::time::{Atime, Clock, Timestamp};
use crate::{Ids, Mode};
use alloc::sync::Arc;

mod recuperation;

#[derive(Clone, Copy)]
struct Entree<'a> {
    parent: u64,
    nom: &'a str,
    id: u64,
}

impl Entree<'_> {
    fn fichier(self, vue: &impl View) -> Result<Object> {
        if !valid_name(self.nom) || live(vue, self.parent)?.stat.kind != Kind::Directory {
            return Err(Error::Invalid("parent ou nom du remplacement"));
        }
        let cle = name_key(self.parent, self.nom);
        let valeur = get(vue, Family::Names, &cle)?.ok_or(Error::NotFound)?;
        if number(&valeur)? != self.id {
            return Err(Error::Invalid("identité du remplacement périmée"));
        }
        let fichier = live(vue, self.id)?;
        if fichier.stat.kind != Kind::File {
            return Err(Error::IsDirectory);
        }
        Ok(fichier)
    }
}

/// Publication commune au déplacement et à la copie dont la candidate est complète.
/// Cette primitive privée au banc ne confère aucune autorité IPC.
fn remplacer<D: Device>(fs: &mut Engine<D>, source: Entree, destination: Entree) -> Result<()> {
    if fs.atime_policy().is_none() || source.id == destination.id {
        return Err(Error::Invalid("remplacement de deux fichiers P9 distincts"));
    }
    fs.transact(false, |tx| {
        let mut nouveau = source.fichier(tx)?;
        let mut ancien = destination.fichier(tx)?;
        let revision = generation(tx)?;

        // Le nom destination reste présent. L'objet évincé emprunte la collecte P9.
        tx.resize_logical(ancien.stat.size, 0)?;
        let cle = ancien.stat.id.to_le_bytes();
        if let Some((premier, _)) = data::first_extent(tx, ancien.stat.id, 0)? {
            ancien.dead = true;
            ancien.parent = 0;
            ancien.name.clear();
            ancien.stat.revision = revision;
            ancien.times.ctime = tx.now;
            save_object(tx, &ancien)?;
            tx.put(Family::Garbage, cle.to_vec(), gc_value(premier))?;
        } else {
            tx.remove(Family::Objects, &cle)?;
        }
        tx.remove(Family::Names, &name_key(source.parent, source.nom))?;
        tx.put(
            Family::Names,
            name_key(destination.parent, destination.nom),
            source.id.to_le_bytes().to_vec(),
        )?;
        nouveau.parent = destination.parent;
        nouveau.name = destination.nom.to_owned();
        nouveau.stat.revision = revision;
        nouveau.times.ctime = tx.now;
        save_object(tx, &nouveau)?;
        touch(tx, source.parent)?;
        if source.parent != destination.parent {
            touch(tx, destination.parent)?;
        }
        Ok(())
    })
}

const IDS: Ids = Ids {
    fs: [7; 16],
    lv: [9; 16],
};
const ANCIEN: &[u8] = b"Premiere destination, persistante";
const NOUVEAU: &[u8] = b"Deuxieme version, entierement synchronisee";

/// Adaptateur du banc de snapshots chiffrés. Absent des bibliothèques clientes.
pub(crate) fn remplacer_deux_fichiers_du_banc<D: Device>(fs: &mut Engine<D>) -> Result<()> {
    let entree = |nom| -> Result<_> {
        Ok(Entree {
            parent: fs.root_id(),
            nom,
            id: fs.stat(nom)?.id,
        })
    };
    let (source, cible) = (entree("source")?, entree("cible")?);
    remplacer(fs, source, cible)
}

struct Horloge(i64);
impl Clock for Horloge {
    fn now(&self) -> Timestamp {
        Timestamp::utc(self.0, 0).unwrap()
    }
}

struct Cas {
    image: SimSnapshot,
    source: Entree<'static>,
    destination: Entree<'static>,
    copie: bool,
}

impl Cas {
    fn neuf(secteur: u32, copie: bool, decaler: bool) -> Result<Self> {
        let disque = SimDevice::new(512, secteur)?;
        let mut fs = Engine::format_p9(disque, IDS, Atime::Relative, Some(Arc::new(Horloge(100))))?;
        fs.mkdir("/sortie")?;
        if decaler {
            fs.create("/temoin")?;
        }
        fs.create("/original")?;
        fs.write("/original", 0, NOUVEAU)?;
        fs.create("/sortie/cible")?;
        fs.write("/sortie/cible", 0, ANCIEN)?;
        let nom = if copie {
            fs.create("/candidate")?;
            fs.write("/candidate", 0, NOUVEAU)?;
            "candidate"
        } else {
            "original"
        };
        let source = Entree {
            parent: fs.root_id(),
            nom,
            id: fs.stat(nom)?.id,
        };
        let destination = Entree {
            parent: fs.stat("/sortie")?.id,
            nom: "cible",
            id: fs.stat("/sortie/cible")?.id,
        };
        Ok(Self {
            image: fs.unmount()?.control().snapshot(),
            source,
            destination,
            copie,
        })
    }

    fn ouvrir(&self) -> Result<(Engine<SimDevice>, SimControl)> {
        let disque = self.image.device();
        let controle = disque.control();
        let mut fs = Engine::mount_current(disque, IDS, Mode::ReadWrite)?;
        fs.set_clock(Arc::new(Horloge(200)));
        Ok((fs, controle))
    }

    fn verifier(&self, fs: &Engine<SimDevice>, publie: bool) -> Result<()> {
        fs.verify()?;
        let cible = fs.stat("/sortie/cible")?;
        let (id, contenu) = if publie {
            (self.source.id, NOUVEAU)
        } else {
            (self.destination.id, ANCIEN)
        };
        assert_eq!(cible.id, id);
        assert_eq!(fs.inspect_id(id, 0, 1024)?, contenu);
        let dates = fs.times_id(id)?;
        assert_eq!(dates.btime, Timestamp::utc(100, 0)?);
        assert_eq!(dates.mtime, Timestamp::utc(100, 0)?);
        assert_eq!(dates.atime, Timestamp::utc(100, 0)?);
        assert_eq!(
            dates.ctime,
            Timestamp::utc(if publie { 200 } else { 100 }, 0)?
        );
        if publie {
            assert!(matches!(fs.stat(self.source.nom), Err(Error::NotFound)));
        } else {
            assert_eq!(fs.stat(self.source.nom)?.id, self.source.id);
        }
        if self.copie || !publie {
            let original = fs.stat("/original")?;
            assert_eq!(fs.inspect_id(original.id, 0, 1024)?, NOUVEAU);
            if self.copie {
                assert_ne!(original.id, self.source.id);
            }
        }
        Ok(())
    }

    fn interrompre(&self, point: usize, persistance: Persistence) -> Result<SimSnapshot> {
        let (mut fs, ctl) = self.ouvrir()?;
        ctl.trace(Some(point), None);
        assert!(remplacer(&mut fs, self.source, self.destination).is_err());
        ctl.power_cut(persistance)?;
        Ok(ctl.snapshot())
    }
}

fn qualifier(secteur: u32, copie: bool, decaler: bool) -> Result<()> {
    let cas = Cas::neuf(secteur, copie, decaler)?;
    let (mut fs, ctl) = cas.ouvrir()?;
    let ancien = fs.receipt();
    ctl.trace(None, None);
    let perimee = Entree {
        id: cas.destination.id + 1000,
        ..cas.destination
    };
    assert!(matches!(
        remplacer(&mut fs, cas.source, perimee),
        Err(Error::Invalid(_))
    ));
    assert!(ctl.events().is_empty());
    cas.verifier(&fs, false)?;
    remplacer(&mut fs, cas.source, cas.destination)?;
    let nouveau = fs.receipt();
    assert_eq!(nouveau.birth, ancien.birth + 1);
    cas.verifier(&fs, true)?;
    let journal = ctl.events();
    let slot = nouveau.birth % 2;
    let ecriture = journal
        .iter()
        .position(|e| *e == Event::WriteSubmitted(slot))
        .unwrap();
    let f2 = journal
        .iter()
        .position(|e| *e == Event::FlushSubmitted(2))
        .unwrap();
    let stable = journal
        .iter()
        .position(|e| *e == Event::FlushEffect(2))
        .unwrap();
    assert!(journal[..ecriture].contains(&Event::FlushCompleted(1)) && ecriture < f2);
    let precedente = Engine::salvage(ctl.snapshot().device(), IDS, 1 - slot)?;
    cas.verifier(&precedente, false)?;
    assert!(fs.collection_pending()?);
    while fs.collect()? {}
    assert!(!fs.collection_pending()?);
    cas.verifier(&fs, true)?;
    assert_eq!(
        fs.accounting()?.logical_bytes,
        (if copie { 2 } else { 1 }) * NOUVEAU.len() as u128
    );

    for point in 0..journal.len() {
        for (persistance, publie) in [
            (Persistence::None, point >= stable),
            (Persistence::Newest(usize::MAX), point >= ecriture),
        ] {
            let image = cas.interrompre(point, persistance)?;
            let relu = Engine::mount_current(image.device(), IDS, Mode::ReadOnly)?;
            assert_eq!(relu.receipt(), if publie { nouveau } else { ancien });
            cas.verifier(&relu, publie)?;
        }
    }
    let masques = 1u16 << (4096 / secteur);
    for masque in 0..masques {
        let image = cas.interrompre(
            f2,
            Persistence::Sectors {
                block: slot,
                mask: masque as u8,
            },
        )?;
        let relu = Engine::mount_current(image.device(), IDS, Mode::ReadOnly)?;
        cas.verifier(&relu, masque & 1 != 0)?;
        if masque < 2 {
            // Nouvelle coupure au FLUSH du remontage RW avant exposition du volume.
            let disque = image.device();
            let controle = disque.control();
            controle.trace(Some(0), None);
            assert!(Engine::mount_current(disque, IDS, Mode::ReadWrite).is_err());
            assert_eq!(controle.events(), [Event::FlushSubmitted(1)]);
            controle.power_cut(Persistence::None)?;
            let reprise = Engine::mount_current(controle.snapshot().device(), IDS, Mode::ReadOnly)?;
            cas.verifier(&reprise, masque == 1)?;
        }
    }
    // Un secteur logique déclaré ne garantit pas son indivisibilité à la coupure.
    let dechiree = cas.interrompre(
        f2,
        Persistence::Prefix {
            block: slot,
            bytes: 96,
        },
    )?;
    for mode in [Mode::ReadOnly, Mode::ReadWrite] {
        let disque = dechiree.device();
        let controle = disque.control();
        controle.trace(None, None);
        assert!(matches!(
            Engine::mount_current(disque, IDS, mode),
            Err(Error::Corrupt(_))
        ));
        assert!(controle.events().is_empty());
        assert!(controle.snapshot().bytes()? == dechiree.bytes()?);
    }
    let diagnostic = dechiree.device();
    let controle = diagnostic.control();
    controle.trace(None, None);
    let secours = Engine::salvage(diagnostic, IDS, 1 - slot)?;
    assert_eq!(secours.receipt(), ancien);
    cas.verifier(&secours, false)?;
    assert!(controle.events().is_empty());
    assert!(controle.snapshot().bytes()? == dechiree.bytes()?);
    println!(
        "secteur={secteur} copie={copie} slot={slot} générations={}/{} frontières={} masques={masques} déchirure=96 montage=refusé secours=ancien",
        ancien.birth,
        nouveau.birth,
        journal.len()
    );
    Ok(())
}

#[test]
fn publication_et_recuperation_p9() -> Result<()> {
    for secteur in [512, 4096] {
        for copie in [false, true] {
            for decaler in [false, true] {
                qualifier(secteur, copie, decaler)?;
            }
        }
    }
    Ok(())
}
