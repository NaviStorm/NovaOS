"""Export QEMU expérimental. Descripteurs Direct uniquement ; portées VFS et
lecteurs non raccordés conservent la validation historique stricte."""
from pathlib import Path

def install(root):
 root=Path(root)
 def edit(name,old,new):
  p=root/name;s=p.read_text()
  if s.count(old)!=1:raise ValueError('ancrage selective EXEC '+name+': '+old[:64])
  p.write_text(s.replace(old,new))
 base='novaOS/kernel/src/sched/process/ipc/'
 p=base+'exec_authority_lab.rs'
 edit(p,'const READ:u64=', 'const ALL:u64=0x4e5847415445414c;\nconst READ:u64=')
 edit(p,'let mut words=[0u64;32];words[28]=r.generation;', '''let mut words=[0u64;48];words[28]=r.generation;
    words[32..35].copy_from_slice(&t.scope);words[35]=u64::from(t.flow.0);
    words[36]=t.binding.object.object;words[37]=t.binding.object.revision;words[38]=t.binding.object.length as u64;words[39]=t.binding.epoch;
    for i in 0..2{words[40+i]=u64::from_le_bytes(t.binding.object.volume[i*8..i*8+8].try_into().unwrap());}
    words[42]=t.binding.app;let issuer=full(s,t.issuer)?;
    for i in 0..4{words[43+i]=u64::from_le_bytes(issuer[i*8..i*8+8].try_into().unwrap());}
    words[47]=t.administrative.map(|(p,_)|p.scope).unwrap_or(0);''')
 edit(p,'    if id==IPC_NOTIFY && args[1]==LIST {','''    if id==IPC_NOTIFY && args[1]==ALL {
        return Some((||{
            let node=s.ipc.lookup(p,args[0],SEND)?;
            if s.ipc.ends.get(node.endpoint.slot()).owner!=p.number{return Err(DENIED);}
            Ok(s.processes.slots.iter().flatten().filter_map(|v|v.exec.first().and_then(|t|
                t.remote.as_ref().filter(|r|r.registry==p && current(s,p).is_ok_and(|v|v.instance==r.registry_instance)
                    && r.leaf>args[2] && r.gate.phase()!=Phase::Settled).map(|r|r.leaf))).min().unwrap_or(0))
        })());
    }
    if id==IPC_NOTIFY && args[1]==LIST {''')
 edit(p,'        let r=s.processes.get_mut(owner).exec[0].remote.as_mut().ok_or(REVOKED)?;', '''        if data[8]==4{validate(s,owner,ticket(s,owner,s.processes.get(owner).exec[0].key)?)?;}
        let r=s.processes.get_mut(owner).exec[0].remote.as_mut().ok_or(REVOKED)?;''')
 edit(p,'        match data[8] {','''        match data[8] {
            4 if value==0 && child==[0;32] && !r.waiting=>r.gate.bind(op).map_err(code)?,''')
 service='novaOS/user/novaauth/service/'
 # Remplacement limité à cette copie de compilation, jamais au noyau ordinaire.
 (root/(service+'src/bin/common/exec_admission_guest.rs')).write_text((root/(service+'src/bin/common/selective_exec_pump.rs')).read_text())
 edit(service+'src/bin/common/recovery_view.rs','admissions.poll().err()','admissions.poll(&state,&mut recovered.storage).err()')
 edit(service+'triplet_effecteur.rs','const TAG:u64=', 'include!("selective_exec_scope_vfs.rs");\nconst TAG:u64=')
 edit(service+'triplet_effecteur.rs','        session.mount_volume()?;', '''        if novaos_identity::exec_file_scope::Probe::supports(&msg.payload) {
            response=scope_seal(session,&msg.payload)?;
            return Ok(());
        }
        session.mount_volume()?;''')
 edit(service+'src/bin/common/recovery_view.rs','            let folder=', '''            if &m.payload[..8]==b"NXEXBND1" {return crate::exec_admission_guest::register(&state,&mut recovered.storage,&m.payload);}
            let folder=''')
 # Seul le vérificateur authentifié produit cette requête ; le CLI n'a aucun
 # accès au port privé et ne fournit ni provenance ni descripteur.
 p=service+'src/bin/common/shell_verifier.rs'
 edit(p,'                if key == 0 {\n                    return Ok(());\n                }','''                if key == 0 {
                    return Ok(());
                }
                let reference=self.live.filter(|(s,_)|s==&id).ok_or(rt::REVOKED)?.1;
                let mut bind=[0;128];bind[..8].copy_from_slice(b"NXEXBND1");bind[8..24].copy_from_slice(&reference);
                bind[24..56].copy_from_slice(&self.route.client);bind[56..88].copy_from_slice(&id);bind[88..96].copy_from_slice(&key.to_le_bytes());
                self.store_seq=self.store_seq.checked_add(1).ok_or(rt::LIMIT)?;
                let registry=administrative::port()?;let result=rpc(registry,registry,self.store_seq,&bind)?;
                if number(&result)!=0{return Err(number(&result));}''')

 edit('novaOS/kernel/build.rs','"--features","membership-r2-guest","--bins"','"--features","selective-exec-guest","--bins"')

def install_file_preflight(root):
 """Raccord de préparation R/NovaFS. Tant que la voie fichier de l'enfant
 n'est pas raccordée, NXRES002 s'arrête avant émission du ticket."""
 root=Path(root)
 def edit(name,old,new):
  p=root/name
  source=p.read_text()
  if source.count(old)!=1:raise ValueError('ancrage portée EXEC '+name+': '+old[:64])
  p.write_text(source.replace(old,new))
 service='novaOS/user/novaauth/service/'
 edit(service+'src/bin/common/recovery_view.rs',
   'let descriptor=state.executable(grant,&key,machine,leaf).map_err(|_|rt::DENIED)?;',
   '''let mut descriptor=state.executable(grant,&key,machine,leaf).map_err(|_|rt::DENIED)?;
                crate::exec_admission_guest::resolve_file(&state,&mut recovered.storage,grant,&mut descriptor,part==1)?;''')
 edit(service+'src/bin/common/shell_verifier.rs',
   '&descriptor[..8]!=b"NXRES001"',
   '!matches!(&descriptor[..8],b"NXRES001"|b"NXRES002")')
 edit(service+'exec_package_read.rs',
   '        if &d[..8]!=b"NXRES001"',
   '''        if &d[..8]==b"NXRES002" {
            // La portée provient de R sur la même conversation que l'image.
            // La phase actuelle observe sa liaison terrain sans octroyer.
            if d[8..40]!=q[56..88] || d[40..56]!=q[8..24] || d[208..232]!=[0;24]
                || d[232..234]!=machine_id() || d[234..]!=[0;6] {
                return Err(rt::DENIED);
            }
            self.mount_volume()?;
            let seal=self.exec_seal()?.ok_or(rt::REVOKED)?;
            let fs=self.fs.as_ref().ok_or(rt::CLOSED)?;
            if d[80..96]!=fs.ids().fs || d[96..112]!=fs.ids().lv || d[184..192]!=d[200..208]
                || seal[8..24]!=d[40..56] || seal[24..40]!=d[184..200] {
                return Err(rt::REVOKED);
            }
            return Err(rt::REFUSED); // aucune capacité fichier sur ce raccord de préparation
        }
        if &d[..8]!=b"NXRES001"''')
 edit(service+'exec_package_read.rs','fn word(b:&[u8])->u64',
   'fn machine_id()->[u8;2] { if cfg!(target_arch="aarch64"){183u16}else{62u16}.to_le_bytes() }\nfn word(b:&[u8])->u64')
