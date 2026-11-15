//! Deux profils terminaux, avec preuve hôte indépendante sur l'état des tables.
//! Le titulaire peut uniquement demander une opération sur sa propre réservation.
use super::*;

impl Bench {
    /// Réservation publiée et capacité active, sans lecture initiale de l'EDU.
    fn reserve_without_io(&mut self, c: &Client, pattern: u8) -> Check<Lease> {
        let id = c.pid();
        c.request(PREPARE, id)?;
        let lease = c
            .while_authorized(|| self.prepare_domain(id, pattern))
            .ok_or("titulaire retire avant reservation")??;
        check(
            self.hw.never_programmed(lease),
            "registres EDU deja programmes",
        )?;
        c.say(ACCEPTED)?;
        Ok(lease)
    }

    pub(super) fn never_submitted(&mut self) -> Check<()> {
        let service = crate::memory::pinned_dma_frames();
        let c = Client::new(LATE)?;
        let id = c.pid();
        let pattern = 0x73;
        let lease = self.reserve_without_io(&c, pattern)?;
        c.request(READ, id)?;
        // Confirmer l'extraction de la trame. Le client quitte ensuite, avant
        // que le superviseur tente de soumettre cette réservation.
        c.say(ACCEPTED)?;
        c.finish()?;
        self.engine
            .close(lease)
            .map_err(|_| "fermeture reservation")?;
        check(
            c.while_authorized(|| self.hw.submit(&self.engine, lease, Transfer::ReadSource))
                .is_none()
                && !self.hw.submit(&self.engine, lease, Transfer::ReadSource)
                && self.hw.never_programmed(lease)
                && self.engine.matches(lease, 0, pattern)
                && self.engine.matches(lease, 1, !pattern),
            "reservation tardive",
        )?;
        self.retire(lease, false)?;
        check(
            self.engine.parked()
                && self.engine.publish(lease).is_err()
                && self.engine.map(lease, 0, ReadByDevice).is_err()
                && self.engine.pinned() == 0
                && crate::memory::pinned_dma_frames() == service,
            "reservation restituee",
        )?;
        hal::log(format_args!(
            "VTD jamais soumise doorbells=0 cadres_clients=0"
        ));
        Ok(())
    }

    pub(super) fn lost_completion(&mut self) -> Check<()> {
        let service = crate::memory::pinned_dma_frames();
        let c = Client::new(DEATH)?;
        let id = c.pid();
        let pattern = 0x73;
        let lease = self.reserve_without_io(&c, pattern)?;
        let frames = self.previous.ok_or("identite des cadres absente")?.1;

        c.request(START_INFLIGHT, id)?;
        check(
            c.while_authorized(|| {
                self.hw.submit(&self.engine, lease, Transfer::ReadSource) && self.hw.running(lease)
            }) == Some(true),
            "demarrage non observe",
        )?;
        c.say(ACCEPTED)?;
        self.engine
            .close(lease)
            .map_err(|_| "cloture fin incertaine")?;
        c.finish()?;

        check(
            until(|| !self.hw.running(lease)),
            "fin moteur avant injection",
        )?;
        check(
            self.hw.erase_completed_length(lease),
            "injection longueur EDU",
        )?;

        // Le résultat du drainage reste incertain même après retour réel de
        // RUN à zéro et relecture de BME fermé. Garder le domaine et ses cadres.
        let mut premature = false;
        let stopped = until(|| {
            premature = self.hw.drain(lease).is_some();
            premature || self.hw.held_without_completion(lease)
        });
        check(
            stopped
                && !premature
                && c.while_authorized(|| ()).is_none()
                && !self.hw.completed(lease)
                && !self.hw.submit(&self.engine, lease, Transfer::ReadSource)
                && self.hw.drain(lease).is_none()
                && self.hw.drain(lease).is_none()
                && self.engine.attach(id).is_err()
                && self.engine.map(lease, 0, ReadByDevice).is_err()
                && self.engine.cancel_preparation(lease).is_err()
                && self.engine.frame_stamp(lease, 0) == Some(frames[0])
                && self.engine.frame_stamp(lease, 1) == Some(frames[1])
                && self.engine.matches(lease, 0, pattern)
                && self.engine.matches(lease, 1, !pattern)
                && self.engine.pinned() == 2
                && crate::memory::pinned_dma_frames() == service + 2,
            "quarantaine appareil non confirme",
        )?;
        hal::log(format_args!(
            "VTD reservation conservee pid={id} epoque={} source={:#x} destination={:#x} motif_source={pattern:#x} motif_destination={:#x}",
            lease.epoch(),
            lease.iova(0),
            lease.iova(1),
            !pattern
        ));
        hal::log(format_args!(
            "VTD fin EDU non confirmee BME=0 cadres_clients=2 cadres_service={service} admissions=0"
        ));
        Ok(())
    }
}
