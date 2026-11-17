//! Effecteur du raccord VFS du profil QEMU. La sélection contient les identifiants
//! d'objets résolus par STAGE. L'installation autorisée est relue dans NovaAuth.
use super::*;
use novaos_identity::{
    Registry,
    lab::Ledger,
    policy::{Image, Origin, READ},
};
pub(super) const BASE: usize = 144;
pub(super) const SELECT: u8 = 0xe8;
pub(super) struct Pending {
    pub(super) key: u64,
    pub(super) grant: delegated::Grant,
}

pub(super) fn check(
    s: &mut Session<'_, '_>,
    key: u64,
    targets: &[selected::Target],
) -> rt::Result<()> {
    if key == 0 || super::exec_files_lab::config(BASE) != key {
        return Err(rt::REVOKED);
    }
    let epoch = super::exec_files_lab::config(BASE + 1);
    let folder = s.redirect_authority(epoch)?;
    let mut owner = [0; 16];
    for (i, chunk) in owner.chunks_exact_mut(8).enumerate() {
        chunk.copy_from_slice(&super::exec_files_lab::config(73 + i).to_le_bytes());
    }
    let mut hash = [0; 32];
    for (i, chunk) in hash.chunks_exact_mut(8).enumerate() {
        chunk.copy_from_slice(&super::exec_files_lab::config(BASE + 4 + i).to_le_bytes());
    }
    let mut ledger = Ledger::new(s.dev.identity_authority().map_err(error)?);
    let fs = s.fs.as_mut().ok_or(rt::CLOSED)?;
    if targets.is_empty() {
        return Err(rt::DENIED);
    }
    for t in targets {
        let file = t.read_only_file().ok_or(rt::DENIED)?;
        if fs.parent_name(file).map_err(error)?.0 != folder {
            return Err(rt::DENIED);
        }
    }
    let root = fs.root_id();
    let marker = fs
        .lookup_at(root, "\u{1f}NXPipeImages")
        .map_err(|_| rt::DENIED)?;
    if marker.kind != Kind::File || marker.size == 0 || marker.size > 768 || marker.size % 48 != 0 {
        return Err(rt::DENIED);
    }
    let records = fs
        .inspect_id(marker.id, 0, marker.size as usize)
        .map_err(error)?;
    let mut found = records.chunks_exact(48).filter(|v| v[16..] == hash);
    let row = found.next().ok_or(rt::DENIED)?;
    if found.next().is_some() {
        return Err(rt::DENIED);
    }
    let image = Image {
        installation: row[..16].try_into().unwrap(),
        digest: hash,
    };
    let reg = Registry::open(fs, &mut ledger, [7; 16]).map_err(|_| rt::REVOKED)?;
    if !reg
        .permission(owner, image, owner, folder, Origin::Direct)
        .is_ok_and(|p| p.rights & READ != 0)
    {
        return Err(rt::DENIED);
    }
    if key != super::exec_files_lab::config(BASE) || epoch != super::exec_files_lab::config(72) {
        return Err(rt::REVOKED);
    }
    Ok(())
}
pub(super) fn prepare(s: &mut Session<'_, '_>, c: &mut Client, q: &Frame) -> rt::Result<Pending> {
    let key = w::word(q, 16);
    if *q != w::frame(SELECT, c.id, key)
        || c.scoped
        || c.closed
        || c.id >> 63 != 0
        || c.private_root.is_some()
    {
        return Err(rt::DENIED);
    }
    check(s, key, &c.targets)?;
    let grant = delegated::selection(s, c, key)?;
    rt::value(rt::call(
        rt::IPC_NOTIFY,
        super::exec_files_lab::config(19),
        0x4e585656414c,
        key,
    ))?;
    Ok(Pending { key, grant })
}
