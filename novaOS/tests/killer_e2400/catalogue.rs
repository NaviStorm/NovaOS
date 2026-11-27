use novaos_gestionnaire_sim::{
    Error as Gestion,
    catalogue::{self, Blob, Bus, Catalog, Policy},
    inventory::{Address, Node, Ownership, Slot},
    lifecycle::State,
    manager::Manager,
    resources::Use,
};
use sha2::{Digest, Sha256};
#[path = "../../user/killer-e2400/profil.rs"]
mod profil;

mod support;
use support::*;

fn complete() -> Policy {
    let mut p = politique(false, 3);
    p.add_resource(RECU, PROFIL, SIG_RECU).unwrap();
    p
}
fn ajouter(c: &mut Catalog<'_>, sig: &[u8], paquet: &[u8]) -> catalogue::Result<[u8; 32]> {
    c.admit(
        MANIFESTE,
        sig,
        &[Blob {
            isa: "x86_64",
            bytes: paquet,
        }],
    )
}
fn catalogue(p: &Policy) -> Catalog<'_> {
    let mut c = Catalog::new(p);
    ajouter(&mut c, SIG, PKG).unwrap();
    c
}
fn noeud() -> Node {
    Node {
        slot: Slot::new(7).unwrap(),
        appearance: 1,
        controller: 1,
        address: Address::Pci {
            segment: 0,
            bus: 1,
            device: 0,
            function: 0,
            express: true,
            bridge: false,
        },
        identity: cible().bus,
        parent: None,
        needs: 0,
        aliases: 0,
        reset: 0,
        irq_scope: 0,
        ownership: Ownership::Independent,
        provider_trusted: true,
    }
}
#[test]
fn paquet_et_plan_figent_lelf_le_profil_et_les_deux_sens() {
    let p = complete();
    let c = catalogue(&p);
    let plan = c.select(&cible(), None).unwrap();
    assert_eq!(plan.driver(), "org.novaos.lab.killer-e2400-5a");
    assert_eq!(plan.capabilities(), &["rx", "tx"]);
    assert_eq!(plan.dependencies().len(), 1);
    assert_eq!(
        plan.dependencies()[0].sha256,
        <[u8; 32]>::from(Sha256::digest(PROFIL))
    );
    assert_eq!(plan.package_digest(), <[u8; 32]>::from(Sha256::digest(PKG)));
    assert_eq!(
        plan.image_digest(),
        <[u8; 32]>::from(Sha256::digest(
            &PKG[68 + u32::from_le_bytes(PKG[..4].try_into().unwrap()) as usize..]
        ))
    );
    let hw = plan.hardware();
    assert!(hw.mmio.is_empty() && hw.irq.is_empty() && hw.max_irq_routes == 0);
    assert_eq!(
        (hw.dma_pages, hw.queues, hw.entries_per_queue),
        (profil::cadres_dma() as u32, 2, 256)
    );
    for r in profil::REGIONS {
        assert!(hw.dma.iter().any(|m| m == r.role));
        assert_eq!(r.role.ends_with(":r"), r.acces == profil::AccesNic::Lecture);
    }
    assert_eq!(hw.dma.len(), profil::REGIONS.len());
    assert!(profil::bases_anneaux_valides([0x1000, 0x2000, 0x3000]));
    assert_eq!(c.select(&cible(), Some(plan.manifest_digest())), Ok(plan));
}
#[test]
fn chaque_element_identite_pci_est_obligatoire() {
    let p = complete();
    let c = catalogue(&p);
    for i in 0..8 {
        let mut t = cible();
        let Bus::Pci {
            vendor,
            device,
            subsystem_vendor,
            subsystem_device,
            revision,
            class_base,
            subclass,
        } = &mut t.bus
        else {
            unreachable!()
        };
        match i {
            0 => *vendor = Some(0x8086),
            1 => *device = Some(0xe091),
            2 => *subsystem_vendor = None,
            3 => *subsystem_device = Some(0x7977),
            4 => *revision = None,
            5 => *class_base = Some(1),
            6 => *subclass = Some(1),
            _ => {
                *subsystem_vendor = None;
                *subsystem_device = None;
            }
        }
        let attendu = if i == 2 {
            catalogue::Error::Format
        } else {
            catalogue::Error::NoMatch
        };
        assert_eq!(c.select(&t, None), Err(attendu), "champ {i}");
    }
}
#[test]
fn ancien_profil_isa_quota_permission_et_epingle_ne_suffisent_pas() {
    let p = complete();
    let c = catalogue(&p);
    for cas in 0..7 {
        let mut t = cible();
        match cas {
            0 => t.features = &["courtier"],
            1 => t.isa = "aarch64",
            2 => t.limits.dma_pages = 258,
            3 => t.profile = Some([9; 32]),
            4 => t.dma = &["tpd:r", "rfd:r", "rrd:r", "tx:r", "rx:w"],
            5 => t.caps = &["rx"],
            _ => t.limits.pending = 7,
        }
        assert!(c.select(&t, None).is_err());
    }
    assert!(c.select(&cible(), Some([0; 32])).is_err());
}
#[test]
fn objet_modifie_ou_confiance_revoquee_sans_publication() {
    let p = complete();
    let mut c = Catalog::new(&p);
    let mut sig = SIG.to_owned();
    sig[0] ^= 1;
    assert_eq!(ajouter(&mut c, &sig, PKG), Err(catalogue::Error::Signature));
    let mut pkg = PKG.to_owned();
    *pkg.last_mut().unwrap() ^= 1;
    assert!(ajouter(&mut c, SIG, &pkg).is_err());
    assert!(c.select(&cible(), None).is_err());
    let mut p = politique(false, 3);
    assert!(p.add_resource(RECU, b"profil substitue", SIG_RECU).is_err());
    assert!(catalogue(&p).select(&cible(), None).is_err());
    for (r, f) in [(true, 3), (false, 4)] {
        let p = politique(r, f);
        assert!(ajouter(&mut Catalog::new(&p), SIG, PKG).is_err());
    }
}
#[test]
fn plan_verifie_259_cadres_refuses_avant_bail_et_lancement() {
    let p = complete();
    let c = catalogue(&p);
    let mut m = Manager::new(1, &c, 1, Use::LAB, Use::default(), None, 0).unwrap();
    let n = noeud();
    let k = n.slot;
    m.refresh(1, true, [n]).unwrap();
    let s = m.discover(k).unwrap();
    let s = m.select(s, &cible(), None, k.bit()).unwrap();
    assert_eq!(m.group(k).unwrap().state(), State::Verified);
    let avant = m.usage().unwrap();
    for _ in 0..3 {
        assert_eq!(m.reserve(s, 16, 1, 1), Err(Gestion::Full));
        assert!(matches!(m.prepare_launch(s), Err(Gestion::Transition)));
        assert!(m.group(k).unwrap().lease().is_none());
        assert_eq!(m.usage(), Ok(avant));
        assert_eq!(m.group(k).unwrap().held(), Use::default());
    }
}
#[test]
fn isolabilite_inconnue_ou_alias_non_autorise_refuse() {
    let p = complete();
    let c = catalogue(&p);
    for cas in 0..3 {
        let mut m = Manager::new(1, &c, 1, Use::LAB, Use::default(), None, 0).unwrap();
        let mut n = noeud();
        let k = n.slot;
        let mut autre = noeud();
        autre.slot = Slot::new(6).unwrap();
        autre.address = Address::Pci {
            segment: 0,
            bus: 1,
            device: 0,
            function: 1,
            express: true,
            bridge: false,
        };
        match cas {
            0 => n.ownership = Ownership::Unknown,
            1 => n.provider_trusted = false,
            _ => n.aliases = autre.slot.bit(),
        }
        m.refresh(1, true, [n, autre]).unwrap();
        let s = m.discover(k).unwrap();
        assert_eq!(m.select(s, &cible(), None, k.bit()), Err(Gestion::Denied));
        assert!(m.group(k).unwrap().lease().is_none());
    }
}
#[test]
fn garde_q35_nvme_precede_toute_reservation() {
    let p = complete();
    let c = catalogue(&p);
    let mut m = Manager::new(1, &c, 1, Use::LAB, Use::default(), Some(1), 0).unwrap();
    let n = noeud();
    let k = n.slot;
    m.refresh(1, true, [n]).unwrap();
    let s = m.discover(k).unwrap();
    let s = m.select(s, &cible(), None, k.bit()).unwrap();
    m.nvme_lease(Some(14)).unwrap();
    assert_eq!(m.reserve(s, 16, 1, 1), Err(Gestion::Denied));
    assert!(m.group(k).unwrap().lease().is_none());
}
