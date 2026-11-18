"""Raccord explicite après le profil récupération VFS ; refus si les ancrages divergent."""
from pathlib import Path

def install(root):
    root=Path(root)
    def edit(name,old,new):
        p=root/name;s=p.read_text()
        if s.count(old)!=1:raise ValueError(f'ancrage admission absent ou ambigu : {name}: {old[:60]}')
        p.write_text(s.replace(old,new))
    base='novaOS/kernel/src/sched/process/ipc/'
    path=base+'exec.rs'
    edit(path,'use super::*;','use super::*;\n#[path="exec_authority_lab.rs"]\nmod authority;\npub(super) use authority::intercept as authority_intercept;')
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
