//! Endpoints de qualification. L'enrôlement et la lecture du facteur
//! empruntent deux canaux indépendants. Aucun octet privé de signature.
use super::*;
use novaos_identity::{Authority, Enrollment, Registry, lab::Ledger};
const DOMAIN: [u8; 16] = [7; 16];
impl Session<'_, '_> {
    pub(super) fn enroll_auth(&mut self, q: &Frame) -> rt::Result<()> {
        if !self.writable
            || self.failed.is_some()
            || self.pending.is_some()
            || self.temporary.any_pending()
            || q[24..].iter().any(|&x| x != 0)
        {
            return Err(rt::REFUSED);
        }
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        let permit = Enrollment::from_provisioner(&mut ledger).map_err(|_| rt::DENIED)?;
        let mut reg = Registry::open(self.fs.as_mut().ok_or(rt::CLOSED)?, &mut ledger, DOMAIN)
            .map_err(|_| rt::REFUSED)?;
        reg.enroll(
            &permit,
            [1; 16],
            "laboratoire",
            q[8..24].try_into().unwrap(),
            "utilisateurs",
        )
        .map(|_| ())
        .map_err(|e| {
            if matches!(
                e,
                novaos_identity::Fault::Uncertain
                    | novaos_identity::Fault::Storage(
                        novafs::Error::Publication { .. } | novafs::Error::UnknownOutcome
                    )
            ) {
                w::UNCERTAIN
            } else {
                rt::REFUSED
            }
        })
    }
    // L'appelant possède l'endpoint privé du vérificateur. Son attestation
    // indique la référence de l'administrateur réellement connecté. Le registre
    // revérifie le statut, le rôle et la tête au dernier point de mutation.
    fn auth_mutate(&mut self, q: &Frame, a: &mut Frame) -> rt::Result<()> {
        if !self.writable
            || self.failed.is_some()
            || self.pending.is_some()
            || self.temporary.any_pending()
        {
            return Err(rt::REFUSED);
        }
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        let head = ledger.current().map_err(|_| rt::REFUSED)?;
        if q[56..88] != head.digest || q[120..].iter().any(|&b| b != 0) {
            return Err(rt::DENIED);
        }
        let mut reg = Registry::open(self.fs.as_mut().ok_or(rt::CLOSED)?, &mut ledger, DOMAIN)
            .map_err(|_| rt::REFUSED)?;
        let owner = reg
            .resolve_verified_factor(q[8..24].try_into().unwrap())
            .map_err(|_| rt::DENIED)?;
        if &q[..8] == b"NA3ISSUE" {
            if !reg
                .inspect(owner, owner.subject())
                .map_err(|_| rt::DENIED)?
                .admin
            {
                return Err(rt::DENIED);
            }
            let h = reg
                .outcome(q[24..40].try_into().unwrap())
                .map_err(|_| rt::REFUSED)?
                .ok_or(w::NOT_FOUND)?;
            a[8..16].copy_from_slice(&h.generation.to_le_bytes());
            return Ok(());
        }
        let argument = q[24..40].try_into().unwrap();
        let request = q[40..56].try_into().unwrap();
        let name = &q[88..120];
        let size = name.iter().position(|b| *b == 0).unwrap_or(name.len());
        if name[size..].iter().any(|&b| b != 0) {
            return Err(rt::BAD_ARGUMENT);
        }
        let name = core::str::from_utf8(&name[..size]).map_err(|_| rt::BAD_ARGUMENT)?;
        let change = match &q[..8] {
            b"NA3CREAT" => {
                let groups = reg.groups(owner).map_err(|_| rt::DENIED)?;
                novaos_identity::Change::Create {
                    name,
                    credential: argument,
                    group: *groups.first().ok_or(rt::DENIED)?,
                }
            }
            b"NA3DISAB" => novaos_identity::Change::Disable(argument),
            b"NA3DELET" => novaos_identity::Change::Delete(argument),
            b"NA3RENAM" => novaos_identity::Change::Rename(argument, name),
            b"NA3UNGRP" => {
                let groups = reg.groups(owner).map_err(|_| rt::DENIED)?;
                novaos_identity::Change::RemoveMember(argument, *groups.first().ok_or(rt::DENIED)?)
            }
            _ => return Err(rt::BAD_ARGUMENT),
        };
        let h = reg.mutate(owner, request, change).map_err(|e| match e {
            novaos_identity::Fault::Uncertain
            | novaos_identity::Fault::Storage(
                novafs::Error::Publication { .. } | novafs::Error::UnknownOutcome,
            ) => w::UNCERTAIN,
            novaos_identity::Fault::Denied => rt::DENIED,
            _ => rt::REFUSED,
        })?;
        if &q[..8] == b"NA3CREAT" {
            a[8..24].copy_from_slice(&novaos_identity::created_user(&h));
        }
        Ok(())
    }
    fn auth_lookup(&mut self, q: &Frame, a: &mut Frame) -> rt::Result<()> {
        if &q[..8] != b"NA3CHECK" || q[24..].iter().any(|&x| x != 0) || self.failed.is_some() {
            return Err(rt::DENIED);
        }
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        let head = ledger.current().map_err(|_| rt::REFUSED)?;
        let reg = Registry::open(self.fs.as_mut().ok_or(rt::CLOSED)?, &mut ledger, DOMAIN)
            .map_err(|_| rt::REFUSED)?;
        let entry = reg
            .resolve_verified_factor(q[8..24].try_into().unwrap())
            .map_err(|_| rt::DENIED)?;
        a[8..24].copy_from_slice(&entry.subject());
        a[24..56].copy_from_slice(&head.digest);
        a[56] = reg
            .groups(entry)
            .map_err(|_| rt::REFUSED)?
            .len()
            .try_into()
            .map_err(|_| rt::LIMIT)?;
        Ok(())
    }
    pub(super) fn serve_auth(&mut self) -> rt::Result<()> {
        let (rx, tx) = (rt::config(18), rt::config(19));
        if rx == 0 || tx == 0 {
            return Ok(());
        }
        let msg = match rt::receive_for(rx, 0) {
            Ok(m) => m,
            Err(rt::WOULD_BLOCK | rt::REVOKED | rt::CLOSED | rt::DENIED) => return Ok(()),
            Err(e) => return Err(e),
        };
        let mut a = [0; 128];
        let status = if msg.kind == 1
            && msg.len == 128
            && matches!(
                &msg.payload[..8],
                b"NA3CREAT" | b"NA3DISAB" | b"NA3DELET" | b"NA3RENAM" | b"NA3UNGRP" | b"NA3ISSUE"
            ) {
            self.auth_mutate(&msg.payload, &mut a)
                .err()
                .unwrap_or(rt::OK)
        } else if msg.kind == 1 && msg.len == 128 {
            self.auth_lookup(&msg.payload, &mut a)
                .err()
                .unwrap_or(rt::OK)
        } else {
            rt::BAD_ARGUMENT
        };
        a[..8].copy_from_slice(&status.to_le_bytes());
        match rt::send_frame(tx, msg.tag, &a, 0, false) {
            Ok(()) | Err(rt::CLOSED | rt::REVOKED | rt::WOULD_BLOCK) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
