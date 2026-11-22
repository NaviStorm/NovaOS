"""Capture chiffrée native après mutation R2, profil de reprise explicitement choisi.

Réutilise Pending.backup et les deux destinataires déjà authentifiés lors de la
récupération. Aucun droit de sauvegarde n'est délégué au CLI. Un échec de capture
ne retire pas le reçu de mutation durable ; le banc refuse alors le redémarrage.
"""
from pathlib import Path

def install(root):
    root=Path(root)
    def edit(name,before,after):
        p=root/name;s=p.read_text()
        if s.count(before)!=1:raise ValueError('ancrage capture R2 : '+name)
        p.write_text(s.replace(before,after))
    base='novaOS/user/novaauth/service/src/bin/common/'
    edit(base+'recovery_view.rs','administration.request_r2(&mut state,&mut recovered.storage,&m.payload,&mut answer)',
         'administration.request_r2(&mut state,&mut recovered.storage,&m.payload,&mut answer,recovered.epoch,recovered.recipients)')
    edit(base+'administration_registry.rs','q:&[u8;128],answer:&mut [u8;128])->rt::Result<()> {',
         'q:&[u8;128],answer:&mut [u8;128],backup_epoch:u64,recipients:[[u8;32];2])->rt::Result<()> {')
    edit(base+'administration_registry.rs','self.transactions.request(state,storage,c,q,answer)',
         'self.transactions.request(state,storage,c,q,answer,backup_epoch,recipients)')
    edit(base+'administration_mutations.rs','c: Context, q: &[u8; 128], answer: &mut [u8; 128]) -> rt::Result<()> {',
         'c: Context, q: &[u8; 128], answer: &mut [u8; 128],backup_epoch:u64,recipients:[[u8;32];2]) -> rt::Result<()> {')
    edit(base+'administration_mutations.rs','                self.pending = None;\n                self.pending_owner = None;',
         '''                // Sonde de reprise : réutiliser le plan déjà confirmé,
                // sans seconde publication administrative ou autorité nouvelle.
                match p.backup(backup_epoch,recipients) {
                    Ok(())=>common::marker(b"NAR2 sauvegarde administrative confirmee"),
                    Err(_)=>common::marker(b"NAR2 sauvegarde administrative non confirmee"),
                }
                self.pending = None;
                self.pending_owner = None;''')
