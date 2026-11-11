//! Oracle de filiation pour le migrateur. Les entrées du journal d’appels du
//! test sont indépendantes des images, des catalogues et de leurs reçus.
use novafs::crypto::{Credential, Secret};
use novafs::device::{SimDevice, SimSnapshot};
use novafs::secure_pool::SecurePool;
use novafs::shared::{ObjectInfo, ObjectKind, Receipt};
use novafs::{Compression, Device, Ids, Result};

const POOL: [u8; 16] = [1; 16];
const OWNER: [u8; 16] = [2; 16];
const CHEMIN: &str = "/d/contenu";
type Pool = SecurePool<SimDevice>;

struct Cles {
    admin: Secret,
    secours: Secret,
    domaine: [u8; 16],
}

impl Cles {
    fn ouvrir(&self, image: &SimSnapshot) -> Result<Pool> {
        let mut p = Pool::open(image.device(), POOL, &self.admin)?;
        p.unlock(self.domaine, Credential::Recovery(&self.secours))?;
        p.check()?;
        Ok(p)
    }
}

struct Observation {
    image: SimSnapshot,
    enfant: u64,
    generation: u64,
    vues: Vec<ObjectInfo>,
    recus: Vec<Receipt>,
}

fn deriver(
    base: &SimSnapshot,
    cles: &Cles,
    parent: u64,
    type_enfant: ObjectKind,
    attendu: &[u8],
) -> Result<Observation> {
    let mut p = cles.ouvrir(base)?;
    let enfant = match type_enfant {
        ObjectKind::Snapshot => p.snapshot(parent)?,
        ObjectKind::Clone => p.clone_snapshot(parent, OWNER)?,
        ObjectKind::Origin => unreachable!("une origine ne se dérive pas"),
    };
    let image = p.into_device().control().snapshot();
    let mut remonte = cles.ouvrir(&image)?;
    novafs::secure_pool::verify_commit_replicas(&image.device(), POOL, &cles.admin)?;
    assert_eq!(remonte.domain(enfant)?, cles.domaine);
    assert_eq!(
        remonte.read_file(enfant, CHEMIN, 0, attendu.len() + 1)?,
        attendu
    );
    let vues = remonte.list()?;
    assert!(vues.iter().any(|v| v.id == enfant && v.kind == type_enfant));
    let mut recus = Vec::new();
    for vue in &vues {
        let recu = remonte.receipt(vue.id)?;
        remonte.verify_receipt(&recu)?;
        recus.push(recu);
    }
    Ok(Observation {
        image,
        enfant,
        generation: remonte.generation(),
        vues,
        recus,
    })
}

fn retirer_parent(
    branche: &Observation,
    parent: u64,
    cles: &Cles,
    contenu: &[u8],
) -> Result<SimSnapshot> {
    let mut p = cles.ouvrir(&branche.image)?;
    let recu_enfant = p.receipt(branche.enfant)?;
    p.remove(parent)?;
    p.advance()?; // Les deux générations protégées sont postérieures au retrait.
    let image = p.into_device().control().snapshot();
    let mut remonte = cles.ouvrir(&image)?;
    assert!(remonte.list()?.iter().all(|v| v.id != parent));
    remonte.verify_receipt(&recu_enfant)?;
    assert_eq!(
        remonte.read_file(branche.enfant, CHEMIN, 0, contenu.len() + 1)?,
        contenu
    );
    Ok(image)
}

fn images_egales(a: &SimSnapshot, b: &SimSnapshot) -> Result<bool> {
    let (a, b) = (a.device(), b.device());
    if a.blocks() != b.blocks() {
        return Ok(false);
    }
    for lba in 0..a.blocks() {
        if a.read(lba)? != b.read(lba)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn audit(secteur: u32) -> Result<()> {
    let disque = SimDevice::new(3000, secteur)?;
    let admin = Secret::from_bytes([21; 32]);
    let secours = Secret::from_bytes([22; 32]);
    let mut p = Pool::format(disque, POOL, &admin)?;
    let ids = Ids {
        fs: [3; 16],
        lv: [4; 16],
    };
    let origine = p.create(
        OWNER,
        ids,
        256,
        Compression::Lz4,
        Some((b"banc P9", &secours)),
    )?;
    let cles = Cles {
        admin,
        secours,
        domaine: p.domain(origine)?,
    };
    p.mkdir(origine, "/d")?;
    p.create_file(origine, CHEMIN)?;
    let contenu = vec![b'N'; 16 * 1024];
    p.write_file(origine, CHEMIN, 0, &contenu)?;
    let s1 = p.snapshot(origine)?;
    let s2 = p.snapshot(origine)?;
    let c1 = p.clone_snapshot(s1, OWNER)?;
    let c2 = p.clone_snapshot(s2, OWNER)?;
    let ctl = p.into_device().control();
    let base = ctl.snapshot();
    for (a, b, kind) in [(s1, s2, ObjectKind::Clone), (c1, c2, ObjectKind::Snapshot)] {
        assert_ne!(a, b);
        let gauche = deriver(&base, &cles, a, kind, &contenu)?;
        let droite = deriver(&base, &cles, b, kind, &contenu)?;
        assert_eq!(gauche.generation, droite.generation);
        assert_eq!(gauche.enfant, droite.enfant);
        assert_eq!(gauche.vues, droite.vues);
        assert_eq!(gauche.recus, droite.recus);
        assert!(images_egales(&gauche.image, &droite.image)?);
        // À gauche seule une autre vue est retirée. À droite le véritable parent disparaît.
        let gauche_apres = retirer_parent(&gauche, b, &cles, &contenu)?;
        let droite_apres = retirer_parent(&droite, b, &cles, &contenu)?;
        assert!(images_egales(&gauche_apres, &droite_apres)?);
        println!(
            "secteur={secteur} enfant={kind:?} parents_appeles={a}/{b} generation={} blocs_identiques={} recus_identiques={} identiques_apres_retrait=oui",
            gauche.generation,
            gauche.image.device().blocks(),
            gauche.recus.len()
        );
    }
    // Témoin discriminant : une seule écriture sur C2 avant de bifurquer.
    let mut p = cles.ouvrir(&base)?;
    let mut autre = contenu.clone();
    autre[4095..4098].copy_from_slice(b"XYZ");
    p.write_file(c2, CHEMIN, 4095, b"XYZ")?;
    let divergente = p.into_device().control().snapshot();
    let gauche = deriver(&divergente, &cles, c1, ObjectKind::Snapshot, &contenu)?;
    let droite = deriver(&divergente, &cles, c2, ObjectKind::Snapshot, &autre)?;
    assert_ne!(gauche.recus, droite.recus);
    assert!(!images_egales(&gauche.image, &droite.image)?);
    assert!(images_egales(&base, &ctl.snapshot())?);
    println!("secteur={secteur} temoin_divergent=distinct source_intacte=oui");
    Ok(())
}

fn main() -> Result<()> {
    for secteur in [512, 4096] {
        audit(secteur)?;
    }
    Ok(())
}
