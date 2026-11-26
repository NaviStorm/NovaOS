//! Collecte privée d'un WAIT de retrait du NIC, lorsque le domaine NVMe reste actif.
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
                    && (self.cookie, self.tail) == (recu.cookie, recu.queue)
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
        let recu = self.duo.retrait.ok_or(Error::Denied)?;
        if (recu.titulaire, recu.generation, recu.cookie) != (id.0, id.1, cookie) {
            return Err(Error::Denied);
        }
        if !self.retrait_verifie(id, bail, plan) {
            return Err(Error::Quarantine);
        }
        self.duo.retrait = None;
        self.duo.delegated = false;
        self.liberer_cadres_nic();
        Ok(())
    }
}
