//! Reset individuel PCI réservé aux profils Ethernet v1 explicites sur AArch64.
use super::*;
type Titulaire = (u64, u64, u64);

pub(super) fn profil() -> bool {
    matches!(option_env!("NOVAOS_ETH3_CASE"), Some("reset-court" | "reset-deces" |
        "reset-revocation" | "reset-incertain" | "reset-differe" | "reset-invalide" |
        "reset-absent" | "reset-sans-option" | "reset-iommu-differe" | "reset-iommu-absent"))
}

pub(super) struct Recu {
    titulaire: Titulaire,
    files: [u16; 2],
}

impl Recu {
    pub(super) fn confirme(self, titulaire: Titulaire, generation: u64) -> bool {
        self.titulaire == titulaire && titulaire.0 != 0 && titulaire.1 != 0 &&
            titulaire.2 == generation && self.files == [0, 1]
    }
}

pub(super) fn retirer(nic: &Device, id: Titulaire) -> Option<Recu> {
    let commun = nic.common;
    let reset = commun.sub(58, 2).ok()?;
    if nic.cfg.get(0, 4) != 0x1041_1af4 || commun.r8(20) != 15 || commun.r16(18) < 2 {
        return None;
    }
    // Le courtier a déjà fermé la racine SEND. La source IRQ est masquée
    // pendant les deux resets. Les pages restent mappées et épinglées.
    nic.cfg.intx(false);
    if nic.cfg.get(4, 2) & 0x406 != 0x406 { return None; }
    let mut recus = [u16::MAX; 2];
    for file in [0, 1u16] {
        commun.w16(22, file);
        if commun.r16(22) != file || commun.r16(28) != 1 || reset.r16(0) != 0 {
            return None;
        }
        if file == 1 && option_env!("NOVAOS_ETH3_CASE") == Some("reset-absent") {
            // Modèle d'une commande non transmise. Le transport indique encore
            // queue_enable=1 pour TX. Le reset global de secours suit séparément.
            hal::log(format_args!("E3E reset TX non transmis queue_enable={}", commun.r16(28)));
            return None;
        }
        reset.w16(0, 1);
        barrier();
        let confirme = || commun.r16(22) == file && reset.r16(0) == 0 && commun.r16(28) == 0;
        wait(confirme).ok()?;
        if file == 1 && option_env!("NOVAOS_ETH3_CASE") == Some("reset-differe") {
            hal::log(format_args!("E3E livraison reset TX differee"));
            crate::sched::sleep(1);
            if !confirme() { return None; }
        }
        recus[usize::from(file)] = commun.r16(22);
        hal::log(format_args!("E3E reset file={file} pid={} generation={} queue_reset=0 queue_enable=0", id.0,id.2));
    }
    let mut recu = Recu { titulaire: id, files: recus };
    if option_env!("NOVAOS_ETH3_CASE") == Some("reset-invalide") {
        recu.titulaire.2 = id.2.checked_sub(1)?;
    }
    Some(recu)
}
