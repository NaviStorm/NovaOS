//! Politique du propriétaire du LV, hors format disque P0. Les adresses du
//! plan privé restent exclusivement empruntées jusqu'au commit ou à l'abandon.
use super::{Allocation, Engine, Plan, RESERVE};
use crate::{Device, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumePolicy {
    pub maintenance_blocks: u64,
    pub logical_quota_bytes: Option<u64>,
}

impl Default for VolumePolicy {
    fn default() -> Self {
        Self {
            maintenance_blocks: RESERVE,
            logical_quota_bytes: None,
        }
    }
}

impl VolumePolicy {
    pub(super) fn validate(self, blocks: u64) -> Result<()> {
        // Un plan occupe au plus 64 nouvelles pages. Les 2 ancres et les 3
        // pages initiales du FS ne sont pas utilisables pour la maintenance.
        if !(64..=4096.min(blocks.saturating_sub(5))).contains(&self.maintenance_blocks) {
            return Err(Error::Invalid("réserve de maintenance hors limites"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservationCost {
    pub data_blocks: u64,
    /// Tous les nœuds CoW, l'index d'allocation et le manifeste.
    /// Le remplacement de l'ancre utilise une adresse fixe déjà comptée.
    pub metadata_blocks: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VolumeAccounting {
    pub capacity_blocks: u64,
    /// Blocs protégés par les générations publiées (live + recoverable).
    pub allocated_blocks: u64,
    /// Tête courante, y compris les deux adresses fixes d'ancrage.
    pub live_blocks: u64,
    /// Blocs retirés de la tête mais encore interdits de recyclage.
    pub recoverable_blocks: u64,
    pub transaction_reserved: u64,
    pub maintenance_reserved: u64,
    pub available_blocks: u64,
    pub maintenance_target: u64,
    /// Somme des longueurs des fichiers vivants. Inclut les trous et exclut
    /// les objets retirés dont la collecte physique est encore en cours.
    pub logical_bytes: u128,
    pub logical_quota_bytes: Option<u64>,
    /// Taille inline + capacités des deux tableaux de l'allocation publiée.
    /// Les pages de données du plan et les arrondis malloc sont hors de ce champ.
    pub allocation_metadata_bytes: usize,
}

impl<D: Device> Engine<D> {
    /// La politique doit être fournie à chaque montage. La diminution du
    /// quota protège les données existantes et n'autorise aucune aggravation.
    pub fn set_volume_policy(&mut self, policy: VolumePolicy) -> Result<()> {
        self.check_health()?;
        policy.validate(self.head.m.blocks)?;
        self.policy = policy;
        Ok(())
    }

    pub(super) fn logical_quota(&self) -> Option<u64> {
        match (
            self.dev.logical_quota_bytes(),
            self.policy.logical_quota_bytes,
        ) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    pub fn accounting(&self) -> Result<VolumeAccounting> {
        self.check_health()?;
        Ok(self.accounted(0))
    }

    fn accounted(&self, reserved: u64) -> VolumeAccounting {
        let stats = self.space_stats();
        // Le plan n'a pris que des adresses éligibles de cette vue exclusive.
        let remaining = stats.free - reserved;
        let maintenance = remaining.min(self.policy.maintenance_blocks);
        let allocated = self.head.m.blocks - stats.free;
        VolumeAccounting {
            capacity_blocks: self.head.m.blocks,
            allocated_blocks: allocated,
            live_blocks: allocated - stats.deferred,
            recoverable_blocks: stats.deferred,
            transaction_reserved: reserved,
            maintenance_reserved: maintenance,
            available_blocks: remaining - maintenance,
            maintenance_target: self.policy.maintenance_blocks,
            logical_bytes: self.alloc.logical_bytes,
            logical_quota_bytes: self.logical_quota(),
            allocation_metadata_bytes: size_of::<Allocation>()
                + self.alloc.nodes.capacity() * size_of::<u64>()
                + self.alloc.space.0.capacity() * size_of::<crate::space::Run>(),
        }
    }
}

/// Une écriture complètement préparée. L'emprunt mutable maintient le même
/// écrivain et la même génération jusqu'à commit. Drop ne fait aucune E/S.
#[must_use = "valider avec commit ou abandonner avec cancel"]
pub struct PreparedWrite<'a, D> {
    pub(super) engine: &'a mut Engine<D>,
    pub(super) plan: Option<Plan>,
}

impl<D: Device> PreparedWrite<'_, D> {
    /// Seul le coordinateur garde le plan hors de l'emprunt, sous ticket exclusif du LV.
    pub(crate) fn detach(self) -> (crate::Ref, Option<Plan>) {
        (self.engine.receipt(), self.plan)
    }
    pub fn cost(&self) -> ReservationCost {
        match &self.plan {
            Some(p) => p.cost,
            None => ReservationCost {
                data_blocks: 0,
                metadata_blocks: 0,
            },
        }
    }

    pub fn accounting(&self) -> VolumeAccounting {
        let cost = self.cost();
        self.engine
            .accounted(cost.data_blocks + cost.metadata_blocks)
    }

    pub fn cancel(self) {}

    pub fn commit(self) -> Result<()> {
        self.engine.publish(self.plan)
    }
}

impl<D: Device> Engine<D> {
    pub(crate) fn commit_ticket(
        &mut self,
        base: crate::Ref,
        plan: Option<Plan>,
    ) -> Result<crate::Ref> {
        self.check_health()?;
        if base != self.receipt() {
            return Err(Error::Invalid("génération du plan périmée"));
        }
        self.publish(plan)?;
        Ok(self.receipt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Ids, Mode, device::SimDevice};

    #[test]
    fn p3_refus_de_maintenance_sur_image_saturee() -> Result<()> {
        for sector in [512, 4096] {
            let ids = Ids {
                fs: [31; 16],
                lv: [39; 16],
            };
            let disk = SimDevice::new(256, sector)?;
            let ctl = disk.control();
            let mut fs = Engine::format(disk, ids)?;
            // Générateur d'une image saturée au-delà des limites d'admission
            // normales. Seule cette fixture peut désactiver le plancher.
            fs.policy.maintenance_blocks = 0;
            fs.create("/remplissage")?;
            let suffix = "v".repeat(245);
            for i in 0..80 {
                let path = format!("/{i:03}{suffix}");
                fs.create(&path)?;
                fs.write(&path, 0, b"garde")?;
            }
            let mut pos = 0;
            loop {
                match fs.write("/remplissage", pos, &[0x6d; 4096]) {
                    Ok(()) => pos += 4096,
                    Err(Error::NoSpace) => break,
                    Err(e) => return Err(e),
                }
            }
            let stable = ctl.snapshot().bytes()?;
            let receipt = fs.receipt();
            let mut fs = Engine::mount(fs.unmount()?, ids, Mode::ReadWrite)?;
            ctl.trace(None, None);
            let stats = fs.accounting()?;
            let victim = format!("/000{suffix}");
            assert_eq!(stats.available_blocks, 0);
            assert!(stats.maintenance_reserved < 10);
            assert!(matches!(
                fs.unlink(&victim),
                Err(Error::MaintenanceExhausted)
            ));
            assert!(ctl.events().is_empty());
            assert_eq!(ctl.snapshot().bytes()?, stable);
            assert_eq!(fs.receipt(), receipt);
            assert_eq!(fs.accounting()?, stats);
            assert_eq!(fs.read(&victim, 0, 5)?, b"garde");
            fs.verify()?;
            println!(
                "P3 maintenance secteur={sector} réserve_restante={} unlink_refusé=1 écritures=0 flushs=0",
                stats.maintenance_reserved
            );
        }
        Ok(())
    }
}
