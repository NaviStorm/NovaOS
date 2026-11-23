//! Inclus exclusivement dans exec.rs de l'export QEMU sélectif.
//! Les installations actuellement résolues par R sont personnelles. Ce contrôle
//! atteste l'absence de dépendance de groupe, sans révoquer ces tickets.
//! Un scope inconnu ou en construction interdit l'acquittement ; il n'est jamais
//! reclassé implicitement comme personnel.
pub(super) fn membership_independent(s:&Core, issuer:Pid, subject:[u8;16], domain:[u8;16])->Res<u64> {
    let mut retained=0u64;
    for p in s.processes.slots.iter().flatten() {
        for ticket in &p.exec {
            if ticket.issuer!=issuer || ticket.binding.holder!=subject || ticket.binding.domain!=domain {continue;}
            if ticket.scope!=[0;3] || ticket.remote.is_none() || ticket.binding.installation==[0;16] {
                return Err(WOULD_BLOCK);
            }
            // Ne changer ni state, ni revoked, ni l'admission distante. Les
            // barrières d'époque et d'incarnation historiques restent actives.
            retained=retained.checked_add(1).ok_or(LIMIT)?;
        }
    }
    Ok(retained)
}
