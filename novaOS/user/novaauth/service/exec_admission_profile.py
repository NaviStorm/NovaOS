"""Raccord explicite après le profil récupération VFS ; refus si les ancrages divergent."""
from pathlib import Path

def install(root):
    root=Path(root)
    def edit(name,old,new):
        p=root/name;s=p.read_text()
        if s.count(old)!=1:raise ValueError(f'ancrage admission absent ou ambigu : {name}: {old[:60]}')
        p.write_text(s.replace(old,new))
    # Diagnostic du tas NovaFS dans la copie QEMU seulement. Comptabiliser les
    # octets vivants distingue une fuite d'un suffixe libre gardé par dlmalloc.
    heap=(root/'novaOS/user/heap.rs').read_text()
    counters='use core::sync::atomic::{AtomicUsize,Ordering};\nstatic LIVE:AtomicUsize=AtomicUsize::new(0);\nstatic BLOCKS:AtomicUsize=AtomicUsize::new(0);\nstatic GROW:AtomicUsize=AtomicUsize::new(0);\nstatic SHRINK:AtomicUsize=AtomicUsize::new(0);\nstatic CALLS:AtomicUsize=AtomicUsize::new(0);\nstatic LAST_START:AtomicUsize=AtomicUsize::new(0);\nstatic LAST_END:AtomicUsize=AtomicUsize::new(0);\nstatic REUSE_CALLS:AtomicUsize=AtomicUsize::new(0);\nstatic REUSE_BYTES:AtomicUsize=AtomicUsize::new(0);\nstatic LAST_LIVE:AtomicUsize=AtomicUsize::new(0);\npub(crate) fn memory_reuse()->[usize;3] {[REUSE_CALLS.load(Ordering::Relaxed),REUSE_BYTES.load(Ordering::Relaxed),LAST_LIVE.load(Ordering::Relaxed)]}\npub(crate) fn memory_snapshot()->[usize;5] {\n    [LIVE.load(Ordering::Relaxed),BLOCKS.load(Ordering::Relaxed),GROW.load(Ordering::Relaxed),SHRINK.load(Ordering::Relaxed),CALLS.load(Ordering::Relaxed)]\n}\nstruct Pages;'
    heap=heap.replace('struct Pages;',counters)
    heap=heap.replace('call(HEAP_RESIZE, old as u64, new as u64, 0).status == OK', 'let ok=call(HEAP_RESIZE,old as u64,new as u64,0).status==OK;\n    if ok {if new>old {GROW.fetch_add((new-old)/4096,Ordering::Relaxed);LAST_START.store(old,Ordering::Relaxed);LAST_END.store(new,Ordering::Relaxed);REUSE_CALLS.store(0,Ordering::Relaxed);REUSE_BYTES.store(0,Ordering::Relaxed);LAST_LIVE.store(0,Ordering::Relaxed);} else {SHRINK.fetch_add((old-new)/4096,Ordering::Relaxed);}}\n    ok')
    heap=heap.replace('unsafe { self.0.lock().malloc(l.size(), l.align()) }', 'let p=unsafe { self.0.lock().malloc(l.size(), l.align()) };\n        if !p.is_null() {LIVE.fetch_add(l.size(),Ordering::Relaxed);BLOCKS.fetch_add(1,Ordering::Relaxed);CALLS.fetch_add(1,Ordering::Relaxed);let low=(p as usize).max(LAST_START.load(Ordering::Relaxed));let high=(p as usize).saturating_add(l.size()).min(LAST_END.load(Ordering::Relaxed));if high>low {REUSE_CALLS.fetch_add(1,Ordering::Relaxed);REUSE_BYTES.fetch_add(high-low,Ordering::Relaxed);LAST_LIVE.fetch_add(high-low,Ordering::Relaxed);}}\n        p')
    heap=heap.replace('unsafe { self.0.lock().free(p, l.size(), l.align()) }', 'unsafe { self.0.lock().free(p, l.size(), l.align()) };\n        LIVE.fetch_sub(l.size(),Ordering::Relaxed);BLOCKS.fetch_sub(1,Ordering::Relaxed);let low=(p as usize).max(LAST_START.load(Ordering::Relaxed));let high=(p as usize).saturating_add(l.size()).min(LAST_END.load(Ordering::Relaxed));if high>low {LAST_LIVE.fetch_sub(high-low,Ordering::Relaxed);}')
    (root/'novaOS/user/novafs/heap_diagnostic.rs').write_text(heap)
    edit('novaOS/user/novafs/main.rs','#[path = "../heap.rs"]','#[path = "heap_diagnostic.rs"]')
    base='novaOS/kernel/src/sched/process/ipc/'
    path=base+'exec.rs'
    edit(path,'use super::*;','use super::*;\n#[path="exec_authority_lab.rs"]\nmod authority;\npub(super) use authority::intercept as authority_intercept;\npub(super) use authority::resources as resource_trace;')
    edit(path,'    binding: Binding,\n}\n\nimpl Permit', '    binding: Binding,\n    authority_head: Option<[u8;32]>,\n}\n\nimpl Permit')
    edit(path,'            binding,\n        })','            binding,\n            authority_head: None,\n        })')
    edit(path,'    state: State,\n}', '    state: State,\n    remote: Option<authority::Remote>,\n}')
    edit(path,'.is_some_and(|t| matches!(t.state, State::Claimed))','.is_some_and(|t| matches!(t.state, State::Claimed) || authority::pending(t))')
    edit(path,'        binding,\n    } = permit;', '        binding,\n        authority_head,\n    } = permit;')
    edit(path,'    // SEND est interprété uniquement', '''    let remote=match authority_head {
        Some(head)=>match authority::attach(s,owner,key,head) {
            Ok(value)=>Some(value),Err(e)=>{s.ipc.close(root);return Err(e);}
        },None=>None,
    };
    // SEND est interprété uniquement''')
    edit(path,'        state: State::Ready(image, config),','        state: State::Ready(image, config),\n        remote,')
    edit(path,'fn invalidate(t: &mut Ticket, reason: u64) {','fn invalidate(t: &mut Ticket, reason: u64) {\n    authority::cancel(t);')
    edit(path,'pub(super) fn acknowledge(s: &mut Core, owner: Pid, key: u64) -> Res<()> {','pub(super) fn acknowledge(s: &mut Core, owner: Pid, key: u64) -> Res<()> {\n    if authority::pending(ticket(s,owner,key)?) {return Err(WOULD_BLOCK);}')
    edit(path,'    let owner = with(|s| s.thread(s.current()).owner.unwrap());','    let owner = with(|s| s.thread(s.current()).owner.unwrap());\n    authority::wait(owner,key)?;')
    edit(path,'        let State::Ready(image, config) = core::mem::replace(&mut t.state, State::Claimed) else {','        authority::start(t)?;\n        let State::Ready(image, config) = core::mem::replace(&mut t.state, State::Claimed) else {')
    edit(path,'                let p = s.processes.get_mut(owner);\n                if role!=2', '                authority::created(s,owner,child.0)?;\n                let p = s.processes.get_mut(owner);\n                if role!=2')
    # A failure after created() would make the receipt wrong: move marking below the last fallible check.
    edit(path,'                authority::created(s,owner,child.0)?;\n','')
    edit(path,'                let wait = WaitRight { target: child.0 };','                authority::created(s,owner,child.0)?;\n                let p = s.processes.get_mut(owner);\n                let wait = WaitRight { target: child.0 };')
    edit(path,'                s.processes.get_mut(owner).exec[0].state = State::Failed(e);','                authority::failed(&mut s.processes.get_mut(owner).exec[0]);\n                s.processes.get_mut(owner).exec[0].state = State::Failed(e);')
    path=base+'exec_delivery_lab.rs'
    edit(path,'    scope: [u64; 3],\n) -> Res<u64> {','    scope: [u64; 3],\n    authority_head:[u8;32],\n) -> Res<u64> {')
    edit(path,'        binding,\n    };','        binding,\n        authority_head:Some(authority_head),\n    };')
    path=base+'exec_snapshot_lab.rs'
    edit(path,'deadline, a.role, a.control, a.scope,','deadline, a.role, a.control, a.scope, a.head,')
    path=base+'session_pipeline_lab.rs'
    edit(path,'        [0; 3],\n    )','        [0; 3],\n        v.head,\n    )')
    path=base+'traffic.rs'
    edit(path,'                session_relay::expire(s);','                if !s.ipc.waits.has(tid.slot) && let Some(r)=exec::authority_intercept(s,p,id,args) {return (r.map(Some),0,p.number);}\n                session_relay::expire(s);')
    path='novaOS/kernel/build.rs'
    edit(path,'"--features","vfs-guest","--bins"','"--features","exec-admission-guest","--bins"')
    # Le témoin EXEC emploie le vrai nsh et son signataire de session existant.
    # Les recettes Alice/Bob VFS conservent leur profil séparé et leurs assertions.
    path=base+'service.rs'
    edit(path,'"/recovery-alice.elf"','"/shell-signer.elf"')
    edit(path,'auth_lab::CLIENT | auth_lab::SECOND | auth_lab::PROVISIONER => auth_lab::connect(s,m,target,id)?,',
        'auth_lab::CLIENT => shell_auth_lab::connect(s,m,target)?, auth_lab::SECOND | auth_lab::PROVISIONER => auth_lab::connect(s,m,target,id)?,')
    path='novaOS/user/novaauth/service/src/bin/verifier.rs'
    edit(path,'all(feature = "shell-guest",not(feature="vfs-guest"))',
        'all(feature = "shell-guest",any(not(feature="vfs-guest"),feature="exec-admission-guest"))')
    edit(path,'any(not(feature = "shell-guest"),feature="vfs-guest")',
        'any(not(feature = "shell-guest"),all(feature="vfs-guest",not(feature="exec-admission-guest")))')
    edit('novaOS/user/service_manager.rs','novaauth/service/recovery_vfs_launcher.rs',
        'novaauth/service/exec_admission_launcher.rs')

    edit('novaOS/user/novafs/files.rs','mod package_lab;', '#[path="../novaauth/service/exec_package_read.rs"]\nmod package_lab;')

    edit('novaOS/kernel/src/sched/process/ipc/traffic.rs','        s.ipc.close(root);\n        return Ok(0);','        s.ipc.close(root);\n        exec::resource_trace(s,owner,"fermeture");\n        return Ok(0);')

    # Mesures seulement : le refus de reprise termine le superviseur et sa
    # famille avant que les destructeurs utilisateur puissent tous acquitter.
    edit(base+'exec.rs','pub(super) use authority::resources as resource_trace;',
        'pub(super) use authority::resources as resource_trace;\npub(in crate::sched::process) use authority::retired_resources;')
    edit(base+'service.rs','            stop_process(s, target, Outcome::Exit(125), now);',
        '            hal::log(format_args!("NAEXEC retrait_famille manager={} pid={} service={} statut=125",pid.number,target.number,child.service_id));\n            stop_process(s, target, Outcome::Exit(125), now);')
    edit('novaOS/kernel/src/sched/process.rs','        release_instance(instance);',
        '        release_instance(instance);\n        with(|s| ipc::exec::retired_resources(s,pid));')

    # Admission de trois opérations seulement pour le client nsh désigné par
    # le superviseur. Les tables de jonction restent privées au PID.
    edit('novaOS/kernel/src/sched/process/threads.rs',
        '    if !with(|s| s.processes.get(owner(s)).application_threads) {',
        '    if !with(|s| s.processes.get(owner(s)).application_threads ||\n        ipc::service::thread_witness(s,owner(s),id)) {')
    edit('components/shell/session_auth_lab.rs','use super::rt;',
        'use super::rt;\n#[path="exec_threads_lab.rs"]\nmod exec_threads;')
    edit('components/shell/session_auth_lab.rs','            if *mode != "recu" {',
        '            if !matches!(*mode,"recu"|"concurrent") {')
    edit('components/shell/session_auth_lab.rs',
        '            let launch = rt::call(super::tty::SPAWN, super::tty::EXEC_TICKET, key, 0);',
        '            let launch = if *mode=="concurrent" {exec_threads::consume(key)?} else {rt::call(super::tty::SPAWN, super::tty::EXEC_TICKET, key, 0)};')

    edit('novaOS/kernel/src/sched/process/ipc/exec.rs','    let Permit {\n',
        '    authority::resources(s,permit.owner,"avant_ticket");\n    let Permit {\n')

    edit('novaOS/kernel/src/sched/process/ipc/service.rs',
        'pub(in crate::sched::process) fn owns_family',
        'pub(in crate::sched::process) fn thread_witness(s:&Core,p:Pid,id:u64)->bool {\n    auth_lab::enabled() && s.processes.manager.is_some_and(|m|m.client==p) &&\n    s.processes.get(p).outcome.is_none() &&\n    matches!(id,THREAD_CREATE|THREAD_EXIT|THREAD_JOIN)\n}\n\npub(in crate::sched::process) fn owns_family')

    edit('components/shell/session_auth_lab.rs',
        '        let key = self.ticket(name, rest, 5000, 0, 0)?;',
        '        let key = self.ticket(name, rest, 5000, if matches!(*mode,"recu"|"concurrent") {1}else{0}, 0)?;')
    edit('components/shell/session_auth_lab.rs',
        '            let (fault, code) = nova_cli::process::wait(right)?;\n            if fault || Self::receipt(key)?',
        '            self.drain(&[pid],None)?;\n            let (fault, code) = nova_cli::process::wait(right)?;\n            if fault || Self::receipt(key)?')
    for suffix in ['\n\n    pub(super) fn pipeline(', '\n    pub(super) fn exercise_relay(']:
        edit('components/shell/session_auth_lab.rs',
            '        let _ = rt::call(rt::IPC_CLOSE, key, 0, 0);\n        result\n    }'+suffix,
            '        let closed = exec_threads::close_ticket(key);\n        result.and_then(|value|closed.map(|_|value))\n    }'+suffix)

    # Une seule présentation historique avant toute authentification ; le garde
    # ordinaire et tous les contrôles natifs restent inchangés.
    edit('components/shell/main.rs','mod session_auth_lab;',
        'mod session_auth_lab;\nmod exec_reference_lab;')
    edit('components/shell/main.rs','            let mut source = input_line?;',
        '            let mut source = input_line?;\n            if let Some(status)=exec_reference_lab::intercept(&source)? {\n                frame.status=status;\n                return Ok(script::Flow::Status(status));\n            }')

    # Raccord ajouté après les transformations historiques, sans changer leurs préimages.
    edit('novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs','                for request in [&f, &name] {','                #[cfg(feature="exec-admission-guest")]\n                {\n                    let reference=self.live.filter(|(session,_)|*session==id).ok_or(rt::DENIED)?.1;\n                    let rport=unsafe {(&*(0x401000 as *const core::sync::atomic::AtomicU64).add(21)).load(core::sync::atomic::Ordering::Acquire)};\n                    if rport==0 {return Err(rt::DENIED);}\n                    let mut descriptor=[0;240];\n                    for part in 0..2 {\n                        let mut q=[0;128];q[..8].copy_from_slice(b"NXRESOL1");q[8..24].copy_from_slice(&reference);\n                        q[24..56].copy_from_slice(&current.head);q[56]=n as u8;q[57..57+n].copy_from_slice(&msg[49..49+n]);q[120]=part as u8;\n                        self.store_seq=self.store_seq.checked_add(1).ok_or(rt::LIMIT)?;\n                        let a=rpc(rport,rport,self.store_seq,&q)?;if number(&a)!=0 {return Err(number(&a));}\n                        descriptor[part*120..(part+1)*120].copy_from_slice(&a[8..]);\n                    }\n                    if &descriptor[..8]!=b"NXRES001" || descriptor[8..40]!=current.head || descriptor[40..56]!=holder {return Err(rt::DENIED);}\n                    self.store_seq=self.store_seq.checked_add(1).ok_or(rt::LIMIT)?;\n                    let a=rpc(port,rt::config(6),self.store_seq,&f)?;if number(&a)!=0 {return Err(number(&a));}\n                    for (part,chunk) in descriptor.chunks(96).enumerate() {\n                        let mut q=[0;128];q[..8].copy_from_slice(b"NXPKDSC1");q[8..24].copy_from_slice(&nonce);\n                        q[24..32].copy_from_slice(&(part as u64).to_le_bytes());q[32..32+chunk.len()].copy_from_slice(chunk);\n                        self.store_seq=self.store_seq.checked_add(1).ok_or(rt::LIMIT)?;\n                        let a=rpc(port,rt::config(6),self.store_seq,&q)?;if number(&a)!=0 {return Err(number(&a));}\n                    }\n                }\n                #[cfg(not(feature="exec-admission-guest"))]\n                for request in [&f, &name] {')

    # Raccord ajouté après les transformations historiques, sans changer leurs préimages.
    edit('novaOS/user/novaauth/service/src/bin/common/recovery_view.rs','            let folder=','            #[cfg(feature="exec-admission-guest")]\n            if &m.payload[..8]==b"NXRESOL1" {\n                let n=m.payload[56] as usize;let part=m.payload[120] as usize;\n                if n==0 || n>63 || part>1 || m.payload[57+n..120].iter().any(|b|*b!=0) || m.payload[121..]!=[0;7] {return Err(rt::BAD_ARGUMENT);}\n                let fresh=crate::tete::query(rt::config(8),&crate::incarnation::identity())?;\n                if fresh[72..80]!=state.head().generation.to_be_bytes() || fresh[80..112]!=state.head().digest || m.payload[24..56]!=state.head().digest {return Err(rt::REVOKED);}\n                let grant=state.resolve_verified_factor(m.payload[8..24].try_into().unwrap()).map_err(|_|rt::DENIED)?;\n                let key=novaos_identity::distribution::public_key(include_bytes!(env!("NOVA_PACKAGE_PUBLIC"))).map_err(|_|rt::DENIED)?;\n                #[cfg(target_arch="aarch64")] let machine=novaos_identity::distribution::Machine::Aarch64;\n                #[cfg(target_arch="x86_64")] let machine=novaos_identity::distribution::Machine::X86_64;\n                let leaf=core::str::from_utf8(&m.payload[57..57+n]).map_err(|_|rt::BAD_ARGUMENT)?;\n                let descriptor=state.executable(grant,&key,machine,leaf).map_err(|_|rt::DENIED)?;\n                answer[8..].copy_from_slice(&descriptor[part*120..(part+1)*120]);return Ok(());\n            }\n            let folder=')
