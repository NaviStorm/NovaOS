    #[cfg(feature = "net-q35-duo")]
    if domain.contention && retirement.is_err() {
        use crate::hardware::nvme::{Command, Error};
        // Requêtes neuves, adressées au contrôleur déjà drainé avant les deux retraits.
        // submit() conserve tous ses contrôles d'état, de CID et de publication.
        let before = b.controller.inflight_cid();
        let started = hal::timer::counter();
        let read = b.controller.submit(Command::Read { nsid: 1, lba: 0, blocks: 1 }, now_ms());
        let flush = b.controller.submit(Command::Flush { nsid: 1 }, now_ms());
        let after = b.controller.inflight_cid();
        hal::log(format_args!("NOVA5L post_panne read={read:?} flush={flush:?} cid_avant={before:?} cid_apres={after:?} cadres={} ticks={}", crate::memory::pinned_dma_frames(), hal::timer::counter().wrapping_sub(started)));
        if before.is_some() || after.is_some() || read != Err(Error::State) || flush != Err(Error::State) {
            return Err("commande apres panne commune");
        }
    }
