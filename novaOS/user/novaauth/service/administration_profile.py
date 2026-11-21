"""R1 additionnel, uniquement dans l'export QEMU après exec_admission_profile.

Les substitutions sont strictes : aucune modification d'une préimage historique,
aucune activation implicite lors d'une construction ordinaire.
"""
from pathlib import Path


def install(root):
    root = Path(root)

    def edit(name, before, after):
        p = root / name
        text = p.read_text()
        if text.count(before) != 1:
            raise ValueError(f"ancrage R1 absent ou ambigu : {name}: {before[:80]}")
        p.write_text(text.replace(before, after))

    base = 'novaOS/kernel/src/sched/process/ipc/'
    path = base + 'session_relay.rs'
    edit(path, 'mod snapshot;', 'mod snapshot;\n#[path="session_admin_lab.rs"]\npub(super) mod administration;\npub(super) use administration::attach as attach_administration;')
    edit(path, '    captures: [Option<display::Capture>; 2],',
         '    captures: [Option<display::Capture>; 2],\n    administrative: [Option<administration::Channel>; 2],')
    edit(path, '        captures: [None; 2],', '        captures: [None; 2],\n        administrative: [None; 2],')
    edit(path, '    display::revoke(s, r);', '    administration::revoke(s,r);\n    display::revoke(s, r);')
    edit(path, '            display::retire_input(s, &mut r, pid);',
         '            administration::retired(s,&mut r,pid);\n            display::retire_input(s, &mut r, pid);')
    edit(path, '    pipeline::tick(s, &mut r);\n    exec::reap_undelivered', '    administration::reap(s,&mut r);\n    pipeline::tick(s, &mut r);\n    exec::reap_undelivered')
    edit(path, '    if let Some(result) = display::intercept(s, route, p, sys, args) {',
         '    if let Some(result)=administration::intercept(s,route,p,sys,args) {return Some(result);}\n    if let Some(result) = display::intercept(s, route, p, sys, args) {')
    path = base + 'exec.rs'
    edit(path, '    remote: Option<authority::Remote>,',
         '    remote: Option<authority::Remote>,\n    administrative:Option<(super::session_relay::administration::Permit,[u8;32])>,')
    edit(path, '        remote,', '        remote,\n        administrative:None,')
    edit(path, '    let header=if config.starts_with(b"NXEXEC01"){64}else{crate::terminal::HEADER};',
         '    let header=if config.starts_with(b"NXADMIN1"){80}else if config.starts_with(b"NXEXEC01"){64}else{crate::terminal::HEADER};')
    edit(path, '                authority::created(s,owner,child.0)?;',
         '                if let Some((permit,head))=ticket(s,owner,key)?.administrative {\n                    let binding=ticket(s,owner,key)?.binding;\n                    super::session_relay::attach_administration(s,owner,child.0,binding,permit,head)?;\n                }\n                authority::created(s,owner,child.0)?;')
    path = base + 'exec_delivery_lab.rs'
    edit(path, '    authority_head:[u8;32],\n) -> Res<u64> {',
         '    authority_head:[u8;32],\n    administrative:Option<super::session_relay::administration::Permit>,\n) -> Res<u64> {')
    edit(path, '    let key = issue(s, permit, image, config, deadline)?;',
         '    if administrative.is_some() && (scope!=[0;3] || config.get(..8)!=Some(b"NXADMIN1")) {return Err(DENIED);}\n    let key = issue(s, permit, image, config, deadline)?;\n    s.processes.get_mut(owner).exec[0].administrative=administrative.map(|p|(p,authority_head));')
    path = base + 'exec_snapshot_lab.rs'
    edit(path, '    sealed: Option<user::SealedImage>,',
         '    sealed: Option<user::SealedImage>,\n    administrative:Option<administration::Permit>,')
    edit(path, '            sealed: None,', '            sealed: None,\n            administrative:None,')
    edit(path, '        b"NXLDISUE" if p == r.issuer => {', '''        b"NXLDADM1" if p == r.issuer => {
            let b=a.binding.ok_or(DENIED)?;
            let scope=word(q,104);let epoch=word(q,112);
            if a.administrative.is_some() || a.sealed.is_none() || a.scope!=[0;3] ||
                q[24..56]!=a.head || q[56..88]!=lease.id || q[88..104]!=b.installation ||
                q[120..]!=[0;8] || scope==0 || scope&!15!=0 || epoch==0 {return Err(DENIED);}
            a.administrative=Some(administration::Permit {scope,epoch});
        }
        b"NXLDISUE" if p == r.issuer => {''')
    edit(path, '                .try_reserve_exact(64 + a.argv_len)',
         '                .try_reserve_exact(if a.administrative.is_some() {80+a.argv_len}else{64+a.argv_len})')
    edit(path, '            config.resize(64, 0);\n            config[..8].copy_from_slice(b"NXEXEC01");',
         '            config.resize(if a.administrative.is_some(){80}else{64},0);\n            config[..8].copy_from_slice(if a.administrative.is_some(){b"NXADMIN1"}else{b"NXEXEC01"});')
    edit(path, '                    lease.limit,\n                );',
         '                    lease.limit,\n                    a.administrative,\n                );')
    edit(path, 'a.scope, a.head,', 'a.scope, a.head, a.administrative,')
    path = base + 'session_pipeline_lab.rs'
    edit(path, '    deadline: u64,\n}', '    deadline: u64,\n    administrative:Option<administration::Permit>,\n}')
    edit(path, '    lease_end: u64,\n) -> Res<u64> {',
         '    lease_end: u64,\n    administrative:Option<administration::Permit>,\n) -> Res<u64> {')
    edit(path, '        deadline,\n    });', '        deadline,\n        administrative,\n    });')
    edit(path, '        v.head,\n    )', '        v.head,\n        v.administrative,\n    )')
    # Le reçu de création du producteur précède son acquittement durable.
    # WOULD_BLOCK à IPC_CLOSE ne signifie pas un échec d'admission du lecteur.
    # Attendre le drainage existant, sans répéter SPAWN ni affaiblir la barrière.
    edit('components/shell/session_auth_lab.rs',
         '            rt::value(rt::call(rt::IPC_CLOSE, key, 0, 0))?;\n            ticket = None;',
         '            exec_threads::close_ticket(key)?;\n            ticket = None;')
    edit('components/shell/session_auth_lab.rs',
         '        if let Some(key) = ticket {\n            let _ = rt::call(rt::IPC_CLOSE, key, 0, 0);\n        }',
         '        let settled = ticket.map(exec_threads::close_ticket).transpose();')
    edit('components/shell/session_auth_lab.rs',
         '        let (producer, consumer) = result?;\n        closed?;',
         '        settled?;\n        let (producer, consumer) = result?;\n        closed?;')
    edit('novaOS/kernel/build.rs', '"--features","exec-admission-guest","--bins"',
         '"--features","administration-guest","--bins"')
    # La route du registre reste celle du vérificateur, sans canal vers NovaFS
    # ni champ d'identité choisi par l'ELF. Un contexte est consommé par requête.
    path = 'novaOS/user/novaauth/service/src/bin/registre-incarnation.rs'
    edit(path, 'mod common;', 'mod common;\n#[path="common/administration_registry.rs"]\nmod administration_registry;')
    path = 'novaOS/user/novaauth/service/src/bin/common/recovery_view.rs'
    edit(path, '    let mut sequence=0;', '    let mut sequence=0;\n    let mut administration=crate::administration_registry::Port::new();')
    edit(path, '            sequence=m.tag;', '''            sequence=m.tag;
            if crate::administration_registry::Port::accepts(&m.payload) {
                let result=administration.request(&state,&m.payload,&mut answer);
                if result.is_err(){administration.clear();}
                return result;
            }
            administration.clear();''')
    edit(path, '            answer[8..24].copy_from_slice(&grant.subject());',
         '            answer[64..72].copy_from_slice(&state.authentication_epoch(grant).map_err(|_|rt::DENIED)?.to_le_bytes());\n            answer[8..24].copy_from_slice(&grant.subject());')
    path = 'novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
    edit(path, 'use novaos_auth_session::sessions::Sessions;',
         'use novaos_auth_session::epoch_sessions::{EpochSessions as Sessions,EpochVerifier as Verifier,EpochBinding as Binding};\n#[path="administration_channel.rs"]\nmod administrative;')
    edit(path, 'struct Login {', 'struct Login {\n    admin_close_step:u64,')
    edit(path, '            auth_trace:AuthTrace::default(),', '            auth_trace:AuthTrace::default(),\n            admin_close_step:0,')
    edit(path, '        self.maintain()?;\n        if self.live.is_some()',
         '        self.maintain().map_err(|e| {administrative::trace(self.admin_close_step,e);e})?;\n        if self.live.is_some()')
    edit(path, '        let r = self.signer(b"NSHPUB01", [0; 32])?;',
         '        let r = self.signer(b"NSHPUB01", [0; 32]).map_err(|e| {administrative::trace(10,e);e})?;')
    edit(path, '        let selected = self.current(reference)?;',
         '        let selected = self.current(reference).map_err(|e| {administrative::trace(11,e);e})?;')
    edit(path, '        if self.relay_bound {\n            match self.relay_control',
         '        self.admin_close_step=1;\n        if self.relay_bound {\n            match self.relay_control')
    edit(path, '        let result = rpc(rt::config(5), rt::config(6), self.store_seq, &drop)?;',
         '        self.admin_close_step=2;\n        let result = rpc(rt::config(5), rt::config(6), self.store_seq, &drop)?;\n        self.admin_close_step=3;')
    edit(path, '        self.last_closed = Some(id);', '        self.last_closed = Some(id);\n        self.admin_close_step=4;')
    edit(path, '        binding(self.store_seq, reference)', '        administrative::binding(self.store_seq, reference)')
    edit(path, '    let mut login = Login::from_launcher();',
         '    let mut login = Login::from_launcher();\n    let mut administrative=administrative::Channels::new();')
    edit(path, '        let m = match rt::receive_for(rt::config(15), 1000) {',
         '        administrative.poll(&mut login);\n        let m = match rt::receive_for(rt::config(15), 2) {')
    edit(path, '                }\n                #[cfg(not(feature="exec-admission-guest"))]\n                for request in [&f, &name] {',
         '                    administrative::authorize_installation(self,nonce,id,&descriptor)?;\n                }\n                #[cfg(not(feature="exec-admission-guest"))]\n                for request in [&f, &name] {')
