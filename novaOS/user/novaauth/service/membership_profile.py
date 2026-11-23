"""Raccord privé sélectif opt-in, appliqué après administration_r2_profile.

Aucun changement du noyau ordinaire. Aucun accroissement du quota ou de droits.
Le protocole de fermeture n'est accessible que sur la conversation R/NovaFS.
"""
from pathlib import Path

def install(root):
    root = Path(root)
    def edit(name, old, new):
        p = root / name
        s = p.read_text()
        if s.count(old) != 1:
            raise ValueError(f'ancrage retrait sélectif absent ou ambigu : {name}: {old[:80]}')
        p.write_text(s.replace(old, new))
    path = 'novaOS/kernel/src/sched/process/ipc/service/auth_lab.rs'
    edit(path, '    config(s, fs, if service == 22 { 464 } else { 465 }, &[remote]);',
         '''    config(s, fs, if service == 22 { 464 } else { 465 }, &[remote]);
    if service == 22 {
        // Identité attribuée par le noyau aux deux extrémités de CETTE route.
        // Un redémarrage de NovaFS ne peut répondre pour l'ancienne instance.
        config(s, owner, 23, &identity(s, fs)?);
        config(s, fs, 468, &identity(s, fs)?);
    }''')
    path = 'novaOS/user/novafs/files/auth_directory.rs'
    edit(path, 'const READ: u8 = 1;',
         '#[path="../../novaauth/service/membership_vfs.rs"]\nmod membership;\nuse membership::*;\nconst READ: u8 = 1;')
    # Inclure le fichier dans le même module : types de leases privés conservés.
    edit(path, '#[path="../../novaauth/service/membership_vfs.rs"]\nmod membership;\nuse membership::*;',
         'include!("../../novaauth/service/membership_vfs_body.rs");')
    source = root/'novaOS/user/novaauth/service/membership_vfs.rs'
    # Rust include! ne permet pas les commentaires intérieurs après un item.
    (source.parent/'membership_vfs_body.rs').write_text(source.read_text().replace('//!', '//'))
    edit(path, '    #[cfg(feature = "politiques-lab")]\n    permission: Option<Permission>,',
         '    permission: Option<Permission>,')
    edit(path, '            #[cfg(feature = "politiques-lab")]\n            permission: None,',
         '            permission: None,')
    edit(path, '    #[cfg(feature = "politiques-lab")]\n    sleeper: Option<(Holder, u64, u64)>,',
         '    sleeper: Option<(Holder, u64, u64)>,\n    membership: Option<GroupClosure>,')
    path = 'novaOS/user/novaauth/service/triplet_effecteur.rs'
    edit(path, '        if !session.writable || session.failed.is_some() || session.pending.is_some() ||',
         '''        if &msg.payload[..8] == b"NXMGVFS1" {
            let mut instance=[0;32];
            for (i,word) in instance.chunks_exact_mut(8).enumerate() {
                word.copy_from_slice(&exec_files_lab::config(468+i).to_le_bytes());
            }
            response=session.directory.membership_close(&msg.payload,instance)?;
            return Ok(());
        }
        if !session.writable || session.failed.is_some() || session.pending.is_some() ||''')
    edit('novaOS/kernel/build.rs', '"--features","administration-r2-guest","--bins"',
         '"--features","membership-r2-guest","--bins"')

    path='novaOS/kernel/src/sched/process/ipc/exec.rs'
    edit(path, 'use super::*;', 'use super::*;\ninclude!("../../../../../user/novaauth/service/exec_membership_body.rs");')
    kernel_source=root/'novaOS/user/novaauth/service/exec_membership_lab.rs'
    (kernel_source.parent/'exec_membership_body.rs').write_text(kernel_source.read_text().replace('//!','//'))
    path='novaOS/kernel/src/sched/process/ipc/session_relay.rs'
    edit(path,'    limit: u64,\n}', '    limit: u64,\n    subject: [u8;16],\n}')
    edit(path,'                limit: deadline,', '                limit: deadline,\n                subject:q[80..96].try_into().unwrap(),')
    edit(path,'        b"NXRSTOP1" => {', '''        b"NXMGEXE1" => {
            let lease=r.live.filter(|l|l.id==id).ok_or(REVOKED)?;
            if q[72..88]!=lease.subject || q[88..104]==[0;16] || word(q,104)>=1<<63
                || q[112..128]==[0;16] {return Err(DENIED);}
            // Aucune décision d'origine sur une image encore en construction.
            if r.admission.is_some() || r.reader.is_some() {return Err(WOULD_BLOCK);}
            exec::membership_independent(s,r.issuer,lease.subject,[7;16])?;
        }
        b"NXRSTOP1" => {''')
    edit(path, 'b"NXRBIND1" | b"NXRSTOP1" | b"NXROFFR1"',
         'b"NXRBIND1" | b"NXRSTOP1" | b"NXROFFR1" | b"NXMGEXE1"')
    edit('novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs',
         'mod administrative;', 'mod administrative;\n#[path="membership_exec.rs"]\nmod membership_exec;')
