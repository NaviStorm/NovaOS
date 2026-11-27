//! Reçu privé du retrait NIC sur la file partagée avec le domaine NVMe.
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Recu {
    titulaire: u64,
    generation: u64,
    cookie: u32,
    queue: u16,
}

impl Engine {
    pub(in crate::hardware) fn retrait_verifie(
        &self,
        id: (u64, u64),
        bail: Lease,
        plan: NvmeLayout,
    ) -> bool {
        let d = &self.duo;
        !d.published
            && (d.owner, d.generation) == id
            && self.closed_nic()
            && super::super::super::pci_ports::port(SID, 4, 2, None) & 4 != 0
            && self.clean_nvme(bail)
            && self.exact_nvme(bail, plan)
            && if let Some(recu) = d.retrait {
                d.delegated
                    && (recu.titulaire, recu.generation) == id
                    && recu.cookie <= self.cookie
                    && u64::from(recu.queue) == (48 * u64::from(recu.cookie)) % 4096
            } else {
                !d.delegated && d.frames.iter().all(|f| f.pa == 0)
            }
    }

    pub(in crate::hardware) fn retirer_individuel(
        &mut self,
        preuve: crate::hardware::nvme_domain::traffic::Drain,
        bail: Lease,
        plan: NvmeLayout,
    ) -> Result<u32> {
        let id = (self.duo.owner, self.duo.generation);
        if !preuve.matches(id.0, id.1)
            || self.duo.retrait.is_some()
            || !self.failed_nic_fenced(id, bail, plan)
        {
            return Err(Error::Quarantine);
        }
        self.invalider_nic(false)?;
        self.duo.retrait = Some(Recu {
            titulaire: id.0,
            generation: id.1,
            cookie: self.cookie,
            queue: self.tail,
        });
        if !self.retrait_verifie(id, bail, plan) {
            return Err(Error::Quarantine);
        }
        Ok(self.cookie)
    }

    pub(in crate::hardware) fn collecter_individuel(
        &mut self,
        id: (u64, u64),
        cookie: u32,
        bail: Lease,
        plan: NvmeLayout,
    ) -> Result<()> {
        let disque_confirme = self.retrait_verifie(id, bail, plan);
        self.consommer_recu(id, cookie, disque_confirme)
    }

    pub(in crate::hardware) fn collecter_disque_retire(
        &mut self,
        id: (u64, u64),
        cookie: u32,
    ) -> Result<()> {
        let disque_confirme =
            self.parked() && super::super::super::pci_ports::port(SID, 4, 2, None) & 4 == 0;
        self.consommer_recu(id, cookie, disque_confirme)
    }

    fn consommer_recu(&mut self, id: (u64, u64), cookie: u32, disque_confirme: bool) -> Result<()> {
        let recu = self.duo.retrait.ok_or(Error::Denied)?;
        if (recu.titulaire, recu.generation, recu.cookie) != (id.0, id.1, cookie) {
            return Err(Error::Denied);
        }
        // Le WAIT suivant peut appartenir au NVMe. Le reçu déjà vérifié reste
        // lié au bail NIC. La file commune doit être saine à son dernier indice.
        let d = &self.duo;
        if !disque_confirme
            || (d.owner, d.generation) != id
            || !d.delegated
            || d.published
            || d.pending.is_some()
            || d.requested.is_some()
            || d.uncertain()
            || self.in_queue
            || recu.cookie > self.cookie
            || u64::from(recu.queue) != (48 * u64::from(recu.cookie)) % 4096
            || !self.closed_nic()
            || !d.verify_nic(&self.service, &self.pages)
            || !d.delivery_ok(self.regs)
            || self.regs.r32(0x34) != 0
            || self.regs.r32(0x38) != 0
            || FAULT_IRQS.load(Ordering::Acquire) != 0
            || self.wait_snapshot() != (self.cookie, u64::from(self.tail), u64::from(self.tail))
        {
            return Err(Error::Quarantine);
        }
        #[cfg(feature = "gestionnaire-4f")]
        if !crate::hardware::net_q35::gestionnaire::autoriser_double(id, cookie, self.duo.frames) {
            return Err(Error::Quarantine);
        }
        self.duo.retrait = None;
        self.duo.delegated = false;
        self.liberer_cadres_nic();
        #[cfg(feature = "gestionnaire-4f")]
        crate::hardware::net_q35::gestionnaire::restitue(id.1);
        Ok(())
    }
}
