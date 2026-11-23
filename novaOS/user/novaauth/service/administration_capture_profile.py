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

    # Une réponse d'autorité perdue passe par reconcile, pas par Confirm.
    # Réutiliser le plan vérifié après activation, jamais submit une seconde fois.
    selective='administration.reconcile_request(' in (root/(base+'recovery_view.rs')).read_text()
    if selective:
        # La preuve de tête reste vérifiée ; ne pas consommer un 17e DEBUG.
        # Les marqueurs avant/après capture restent présents pour les ressources.
        edit(base+'tete.rs','    common::marker(b"NAUTH tete signature et portee verifiees");','')
        edit(base+'recovery_view.rs','administration.reconcile_request(&mut state,&mut recovered.storage,&m.payload)',
             'administration.reconcile_request(&mut state,&mut recovered.storage,&m.payload,recovered.epoch,recovered.recipients)')
        for name in ('administration_registry.rs','administration_mutations.rs'):
            edit(base+name,'q:&[u8;128])->rt::Result<()> {',
                 'q:&[u8;128],backup_epoch:u64,recipients:[[u8;32];2])->rt::Result<()> {')
        edit(base+'administration_registry.rs','self.transactions.reconcile_request(state,storage,q)',
             'self.transactions.reconcile_request(state,storage,q,backup_epoch,recipients)')
        edit(base+'administration_mutations.rs','self.reconcile(state,storage)',
             'self.reconcile(state,storage,backup_epoch,recipients)')
    else:
        edit(base+'recovery_view.rs','administration.reconcile(&mut state,&mut recovered.storage)?;',
             'administration.reconcile(&mut state,&mut recovered.storage,recovered.epoch,recovered.recipients)?;')
    edit(base+'administration_registry.rs','pub fn reconcile(&mut self,state:&mut View,storage:&mut crate::recovery_store::Storage)->rt::Result<()> {',
         'pub fn reconcile(&mut self,state:&mut View,storage:&mut crate::recovery_store::Storage,backup_epoch:u64,recipients:[[u8;32];2])->rt::Result<()> {')
    edit(base+'administration_registry.rs','self.transactions.reconcile(state,storage)',
         'self.transactions.reconcile(state,storage,backup_epoch,recipients)')
    edit(base+'administration_mutations.rs','pub fn reconcile(&mut self, state: &mut View, storage: &mut Storage) -> rt::Result<()> {',
         'pub fn reconcile(&mut self, state: &mut View, storage: &mut Storage,backup_epoch:u64,recipients:[[u8;32];2]) -> rt::Result<()> {')
    edit(base+'administration_mutations.rs','            let owner = self.pending_owner.ok_or(UNCERTAIN)?;',
         '            match p.backup(backup_epoch,recipients) {\n                Ok(())=>common::marker(b"NAR2 sauvegarde administrative confirmee"),\n                Err(_)=>common::marker(b"NAR2 sauvegarde administrative non confirmee"),\n            }\n            let owner = self.pending_owner.ok_or(UNCERTAIN)?;')
