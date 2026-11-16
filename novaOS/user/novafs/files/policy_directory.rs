use novaos_identity::{
    Change, Fault, Head, created_group, created_user,
    policy::{self, Image, Origin, Permission},
};
fn map_policy(e: Fault) -> u64 {
    match e {
        Fault::Uncertain => w::UNCERTAIN,
        Fault::Storage(e) => error(e),
        Fault::Conflict => rt::REFUSED,
        Fault::Denied => rt::DENIED,
        _ => rt::REFUSED,
    }
}
fn attested_image(lane: usize) -> Image {
    let mut bytes = [0u8; 48];
    for (i, b) in bytes.chunks_exact_mut(8).enumerate() {
        let v = unsafe {
            (&*(0x401000 as *const core::sync::atomic::AtomicU64).add(32 + 6 * lane + i))
                .load(core::sync::atomic::Ordering::Acquire)
        };
        b.copy_from_slice(&v.to_le_bytes());
    }
    Image {
        installation: bytes[..16].try_into().unwrap(),
        digest: bytes[16..].try_into().unwrap(),
    }
}
impl Directory {
    fn policy_root(&self, h: Holder, mut k: u64) -> rt::Result<Permission> {
        for _ in &self.leases {
            let l = self
                .leases
                .iter()
                .flatten()
                .find(|l| l.holder == h && l.key == k)
                .ok_or(rt::REVOKED)?;
            if l.parent == 0 {
                return l.permission.ok_or(rt::REVOKED);
            }
            k = l.parent;
        }
        Err(rt::REVOKED)
    }
}
impl Session<'_, '_> {
    // Le port du lanceur est privé. Il veut le même couple installation/empreinte
    // que celui que le noyau vérifiera au dernier point précédant image().
    pub(super) fn policy_start(&mut self, q: &Frame, a: &mut Frame) -> rt::Result<()> {
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        let h = ledger.current().map_err(map_policy)?;
        let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
        let dir = fs.lookup_at(fs.root_id(), NAME).map_err(error)?;
        let f = fs.lookup_at(dir.id, "politique").map_err(error)?;
        let seal: [u8; 104] = fs
            .inspect_id(f.id, 0, 104)
            .map_err(error)?
            .try_into()
            .map_err(|_| w::CORRUPT)?;
        let reg = Registry::open(fs, &mut ledger, [7; 16]).map_err(map_policy)?;
        let authority = |request: u8| -> rt::Result<Head> {
            reg.outcome([request; 16])
                .map_err(map_policy)?
                .ok_or(rt::REFUSED)
        };
        let owner = created_user(&authority(1)?);
        let install = policy::installed(&authority(4)?);
        // Découverte séparée, accessible exclusivement au lanceur.
        if q[8] == 0 {
            a[24..40].copy_from_slice(&install);
            a[40..48].copy_from_slice(&h.generation.to_le_bytes());
            return Ok(());
        }
        if q[8] != 1 || q[16..32] != install {
            return Err(rt::DENIED);
        }
        reg.verify_image(Image {
            installation: install,
            digest: q[32..64].try_into().unwrap(),
        })
        .map_err(map_policy)?;
        reg.verify_folder(owner, dir.id, &seal)
            .map_err(map_policy)?;
        if seal[8..24] != owner || dir.id != number(&seal[56..]) || f.id != number(&seal[88..]) {
            return Err(w::CORRUPT);
        }
        self.directory.seal = Some(seal);
        self.directory.group = created_group(&authority(3)?);
        self.directory.install = install;
        Ok(())
    }
    pub(super) fn policy_change(&mut self, q: &Frame, a: &mut Frame) -> rt::Result<()> {
        let seal = self.directory.seal.ok_or(rt::DENIED)?;
        let owner = seal[8..24].try_into().unwrap();
        let object = number(&seal[56..]);
        let group = self.directory.group;
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        let h = ledger.current().map_err(map_policy)?;
        if q[32..64] != h.digest {
            return Err(rt::REFUSED);
        }
        let mut reg = Registry::open(self.fs.as_mut().ok_or(rt::CLOSED)?, &mut ledger, [7; 16])
            .map_err(map_policy)?;
        let subject = reg
            .resolve_verified_factor(q[16..32].try_into().unwrap())
            .map_err(map_policy)?;
        let origin = match q[64] {
            0 => Origin::Direct,
            1 => Origin::Group(group),
            _ => return Err(rt::BAD_ARGUMENT),
        };
        let request = q[80..96].try_into().unwrap();
        match q[8] {
            12 => {
                a[8] = reg
                    .stored_permission(subject, object, self.directory.install, origin)
                    .map_err(map_policy)?;
            }
            13 => {
                reg.set_permission(
                    subject,
                    owner,
                    object,
                    self.directory.install,
                    origin,
                    q[65],
                    request,
                )
                .map_err(map_policy)?;
            }
            14 | 15 => {
                let change = if q[8] == 14 {
                    Change::AddMember(owner, group)
                } else {
                    Change::RemoveMember(owner, group)
                };
                reg.mutate(subject, request, change).map_err(map_policy)?;
            }
            _ => return Err(rt::BAD_ARGUMENT),
        }
        // Sérialisation avec l'admission VFS. Réévaluer seulement les racines
        // existantes selon leur origine. Les branches directes restent utilisables.
        if q[8] != 12 {
            for slot in 0..self.directory.leases.len() {
                let Some(l) = self.directory.leases[slot] else {
                    continue;
                };
                let Some(old) = l.permission else { continue };
                let lane = usize::from(l.holder.port == port(1, false));
                if reg
                    .permission(
                        l.holder.subject,
                        attested_image(lane),
                        owner,
                        object,
                        old.origin,
                    )
                    .ok()
                    != Some(old)
                {
                    self.directory.revoke(l.holder, l.key)?;
                }
            }
        }
        Ok(())
    }
    fn policy_operation(&mut self, lane: usize, holder: Holder, q: &Frame) -> rt::Result<u64> {
        let seal = self.directory.seal.ok_or(rt::DENIED)?;
        let owner = seal[8..24].try_into().unwrap();
        let oid = number(&seal[56..]);
        let key = number(&q[8..]);
        let opening = matches!(q[0], 1 | 10);
        let old = if opening {
            None
        } else {
            Some(self.directory.policy_root(holder, key)?)
        };
        let origin = old.map_or_else(
            || {
                if q[0] == 10 {
                    Origin::Group(self.directory.group)
                } else {
                    Origin::Direct
                }
            },
            |l| l.origin,
        );
        let mut ledger = Ledger::new(self.dev.identity_authority().map_err(error)?);
        let reg = Registry::open(self.fs.as_mut().ok_or(rt::CLOSED)?, &mut ledger, [7; 16])
            .map_err(map_policy)?;
        let permission = reg
            .permission(holder.subject, attested_image(lane), owner, oid, origin)
            .map_err(|e| {
                if old.is_some() {
                    rt::REVOKED
                } else {
                    map_policy(e)
                }
            })?;
        if old.is_some_and(|old| old != permission) {
            return Err(rt::REVOKED);
        }
        if opening {
            if key != 0
                || q[1] == 0
                || q[1] & !permission.rights != 0
                || q[2..].iter().any(|b| *b != 0)
            {
                return Err(rt::DENIED);
            }
            // Recontrôler l'ACL réelle avant de délivrer le premier handle.
            let fs = self.fs.as_mut().ok_or(rt::CLOSED)?;
            let f = fs.lookup_at(oid, "politique").map_err(error)?;
            if fs.inspect_id(f.id, 0, 104).map_err(error)? != seal || q[1] & !seal[96] != 0 {
                return Err(rt::DENIED);
            }
            let key = self.directory.grant(holder, 0, q[1])?;
            self.directory
                .leases
                .iter_mut()
                .flatten()
                .find(|l| l.key == key)
                .ok_or(rt::REFUSED)?
                .permission = Some(permission);
            return Ok(key);
        }
        let mut command = *q;
        if q[0] == 11 {
            command[0] = 5;
        } // Préparation dont le client attend l'issue.
        self.folder_op(holder, &command, true)
    }
}
