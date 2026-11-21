//! Raccord privé du seul export QEMU exec-admission. Aucun appel public ajouté.
use super::*;
#[path="exec_gate_state.rs"]
mod state;
use state::{Gate,Phase};
const LIST:u64=0x4e58474154454c53;
const READ:u64=0x4e58474154450000;
fn code(e:state::Error)->u64 {match e {state::Error::Busy=>WOULD_BLOCK,state::Error::Invalid=>BAD_ARGUMENT,_=>REFUSED}}
fn full(s:&Core,p:Pid)->Res<[u8;32]> {
    current(s,p)?.instance.identity(crate::boot_id::current()).map(|i|i.encode()).ok_or(REVOKED)
}
pub(super) struct Remote {
    registry:Pid,
    registry_instance:Instance,
    leaf:u64,
    head:[u8;32],
    waiting:bool,
    generation:u64,
    gate:Gate,
}
pub(super) fn attach(s:&mut Core,owner:Pid,root:u64,head:[u8;32])->Res<Remote> {
    if head==[0;32] {return Err(DENIED);}
    let manager=s.processes.manager.ok_or(DENIED)?.manager;
    let mut registries=s.processes.slots.iter().flatten().filter(|p|
        p.service_owner==Some(manager) && p.service_id==22 && p.outcome.is_none() && p.space.is_some());
    let registry=registries.next().ok_or(CLOSED)?.pid;
    if registries.next().is_some() {return Err(DENIED);}
    let registry_instance=current(s,registry)?.instance;
    let leaf=s.ipc.derive(owner,root,registry,(SEND|RECEIVE).into())?;
    Ok(Remote {registry,registry_instance,leaf,head,waiting:false,generation:0,gate:Gate::new()})
}
pub(super) fn pending(t:&Ticket)->bool {t.remote.as_ref().is_some_and(|r|!r.gate.detachable())}
pub(super) fn cancel(t:&mut Ticket) {
    if let Some(r)=t.remote.as_mut() {
        // LIST et la transition request exigent waiting=true. Avant ce point,
        // invalidate rend le ticket inutilisable sous Core et aucune opération
        // distante ne peut exister : la porte Dormant est déjà détachable.
        // Garder Cancelled -> Settled dès que la pompe peut avoir vu le ticket.
        if r.waiting {let _=r.gate.cancel_before_creation();}
    }
}
#[inline(never)]
pub(super) fn start(t:&mut Ticket)->Res<()> {
    if let Some(r)=t.remote.as_mut() {r.gate.start().map_err(code)?;}Ok(())
}
pub(super) fn created(s:&mut Core,owner:Pid,child:Pid)->Res<()> {
    let identity=full(s,child)?;
    if let Some(r)=s.processes.get_mut(owner).exec[0].remote.as_mut() {r.gate.created(identity).map_err(code)?;}
    hal::log(format_args!("NAEXEC enfant owner={} child={}",owner.number,child.number));
    Ok(())
}
pub(super) fn failed(t:&mut Ticket) {
    if let Some(r)=t.remote.as_mut() {
        // Seulement après retire_unpublished et la fin de la construction locale.
        if r.gate.phase()==Phase::Creating {let _=r.gate.failed_after_cleanup();}
        else {let _=r.gate.cancel_before_creation();}
    }
}
pub(super) fn wait(owner:Pid,key:u64)->Res<()> {
    let required=with(|s| {
        resources(s,owner,"consommation");
        let result=(|| {
        let t=ticket(s,owner,key)?;
        hal::log(format_args!("NAEXEC attente owner={} ticket={key} maintenant={} echeance={}",owner.number,ticks(),t.deadline));
        validate(s,owner,t)?;
        let Some(r)=t.remote.as_ref() else {return Ok(false);};
        if r.waiting {return Err(REFUSED);}
        if current(s,r.registry)?.instance!=r.registry_instance {return Err(REVOKED);}
        s.processes.get_mut(owner).exec[0].remote.as_mut().unwrap().waiting=true;
        Ok(true)
        })();
        // Les compteurs endpoints/nodes/waits sont globaux. Garder Core
        // verrouillé entre les deux mesures du refus : l'activité d'un autre
        // processus ne doit pas être attribuée à cet appel sans création.
        if result.is_err() {resources(s,owner,"refus_sans_creation");}
        result
    });
    if !required? {return Ok(());}
    loop {
        let ready=with(|s| {
            let t=ticket(s,owner,key)?;
            if let Err(e)=validate(s,owner,t) {
                hal::log(format_args!("NAEXEC refus owner={} ticket={key} statut={e} maintenant={} echeance={}",owner.number,ticks(),t.deadline));
                invalidate(&mut s.processes.get_mut(owner).exec[0],e);return Err(e);
            }
            let r=t.remote.as_ref().ok_or(REVOKED)?;
            if current(s,r.registry)?.instance!=r.registry_instance {return Err(REVOKED);}
            match r.gate.phase() {
                Phase::Admitted=>Ok(true),
                Phase::Dormant|Phase::Requested=>Ok(false),
                _=>{hal::log(format_args!("NAEXEC refus owner={} ticket={key} statut={REVOKED} maintenant={} echeance={}",owner.number,ticks(),t.deadline));Err(REVOKED)},
            }
        })?;
        if ready {return Ok(());}
        dispatch(Reason::Yield);
    }
}
fn locate(s:&Core,registry:Pid,key:u64)->Option<Pid> {
    s.processes.slots.iter().flatten().find_map(|p|p.exec.first().and_then(|t|
        t.remote.as_ref().filter(|r|r.registry==registry && r.leaf==key && r.leaf!=0 &&
            current(s,registry).is_ok_and(|p|p.instance==r.registry_instance)).map(|_|p.pid)))
}
fn metadata(s:&Core,owner:Pid,index:usize)->Res<u64> {
    let t=ticket(s,owner,s.processes.get(owner).exec[0].key)?;
    let r=t.remote.as_ref().ok_or(DENIED)?;
    let mut words=[0u64;29];words[28]=r.generation;
    words[0]=match r.gate.phase(){Phase::Dormant=>0,Phase::Requested=>1,Phase::Admitted=>2,
        Phase::Creating=>3,Phase::Created=>4,Phase::Cancelled=>5,Phase::Settled=>6};
    words[1]=t.key;words[2]=r.gate.position();words[3]=t.deadline;
    for (start,bytes) in [(4,full(s,owner)?),(8,t.binding.session),(14,t.binding.digest),(18,r.head)] {
        for i in 0..4 {words[start+i]=u64::from_le_bytes(bytes[8*i..8*i+8].try_into().unwrap());}
    }
    for i in 0..2 {words[12+i]=u64::from_le_bytes(t.binding.installation[8*i..8*i+8].try_into().unwrap());}
    if let Ok((_,child))=r.gate.terminal() {
        for i in 0..4 {words[22+i]=u64::from_le_bytes(child[8*i..8*i+8].try_into().unwrap());}
    }
    let op=r.gate.operation();for i in 0..2 {words[26+i]=u64::from_le_bytes(op[8*i..8*i+8].try_into().unwrap());}
    words.get(index).copied().ok_or(BAD_ARGUMENT)
}
pub(in crate::sched::process::ipc) fn intercept(s:&mut Core,p:Pid,id:u64,args:[u64;3])->Option<Res<u64>> {
    let manager=s.processes.manager?;
    let process=s.processes.get(p);
    if process.service_id!=22 || process.service_owner!=Some(manager.manager) || process.outcome.is_some() {return None;}
    if id==IPC_NOTIFY && args[1]==LIST {
        return Some((|| {
            if args[2]!=0 {return Err(BAD_ARGUMENT);}
            let node=s.ipc.lookup(p,args[0],SEND)?;
            if s.ipc.ends.get(node.endpoint.slot()).owner!=p.number {return Err(DENIED);}
            Ok(s.processes.slots.iter().flatten().find_map(|v|v.exec.first().and_then(|t|
                t.remote.as_ref().filter(|r|r.registry==p && r.waiting && r.leaf!=0 && r.gate.phase()!=Phase::Settled)
                    .map(|r|r.leaf))).unwrap_or(0))
        })());
    }
    let owner=locate(s,p,args[0])?;
    if id==IPC_NOTIFY && args[1]&!255==READ {
        return Some((|| {
            s.ipc.lookup(p,args[0],RECEIVE)?;
            if args[2]!=0 {return Err(BAD_ARGUMENT);}metadata(s,owner,(args[1]&255) as usize)
        })());
    }
    if id!=IPC_SEND {return Some(Err(DENIED));}
    Some((|| {
        s.ipc.lookup(p,args[0],SEND)?;
        let space=current(s,p)?.space.as_ref().ok_or(REVOKED)?;
        let q=super::super::traffic::input::<160>(space,args[1],args[2],160,false)?;
        let data=&q[32..];
        let timeout=super::super::traffic::word(&q,8);
        let flags=super::super::traffic::dword(&q,28);
        if flags>1 || timeout>15000 || (flags==0)!=(timeout==0) ||
            super::super::traffic::word(&q,16)!=0 {return Err(BAD_ARGUMENT);}
        if super::super::traffic::dword(&q,24)!=128 || &data[..8]!=b"NAXGATE1" ||
            data[9..16]!=[0;7] || data[72..]!=[0;56] {return Err(BAD_ARGUMENT);}
        let op:[u8;16]=data[16..32].try_into().unwrap();
        let value=u64::from_le_bytes(data[32..40].try_into().unwrap());
        let child:[u8;32]=data[40..72].try_into().unwrap();
        let r=s.processes.get_mut(owner).exec[0].remote.as_mut().ok_or(REVOKED)?;
        match data[8] {
            0 if value<1<<63 && child==[0;32] && r.waiting=>{r.gate.request(op).map_err(code)?;r.generation=value;},
            1 if child==[0;32]=>r.gate.admit(op,value).map_err(code)?,
            2 if op==r.gate.operation() && value==0 && child==[0;32]=>r.gate.cancel_before_creation().map_err(code)?,
            3 if matches!(value,1|2)=>{
                r.gate.settle(op,value as u8,child).map_err(code)?;
                let leaf=r.leaf;r.leaf=0;
                let node=s.ipc.lookup(p,leaf,0)?;s.ipc.close(node);
            }
            _=>return Err(DENIED),
        }
        hal::log(format_args!("NAEXEC transition registry={} owner={} kind={} value={value}",p.number,owner.number,data[8]));
        Ok(128)
    })())
}

pub(in crate::sched::process::ipc) fn resources(s:&Core,owner:Pid,label:&str) {
    let (endpoints,nodes,handles,waits)=s.ipc.budget_usage(owner);
    let live=s.processes.slots.iter().flatten().filter(|p|p.outcome.is_none()).count();
    let threads=s.slots.iter().flatten().filter(|t|t.owner==Some(owner)).count();
    let joins=s.processes.get(owner).join_rights.len();
    let free=heap::with_pages(|m|Ok(m.free_pages()));
    hal::log(format_args!("NAEXEC ressources phase={label} owner={} roots={} handles={handles} endpoints={endpoints} nodes={nodes} waits={waits} processus={live} threads={threads} joins={joins} cadres_ipc={} cadres_libres={free:?}",owner.number,s.ipc.root_count(owner.number),s.ipc.pages()));
    // Sonde QEMU de diagnostic : attribuer une variation globale aux espaces
    // réellement résidents, sans modifier l'allocateur ni normaliser le tas.
    for p in s.processes.slots.iter().flatten().filter(|p|p.outcome.is_none()) {
        let (pages,stack)=p.space.as_ref().map(|v|v.page_counts()).unwrap_or((0,0));
        hal::log(format_args!("NAEXEC resident phase={label} pid={} service={} pages={pages} pile={stack}",p.pid.number,p.service_id));
    }
    for process in s.processes.slots.iter().flatten().filter(|p|
        p.outcome.is_none() && (p.pid==owner || matches!(p.service_id,22|23))) {
        let pid=process.pid;
        let (roots,handles,waits,ipc)=s.ipc.tls_snapshot(pid);
        let (pages,stack)=process.space.as_ref().map(|v|v.page_counts()).unwrap_or((0,0));
        let threads=s.slots.iter().flatten().filter(|t|t.owner==Some(pid)).count();
        hal::log(format_args!("NAEXEC acteur phase={label} owner={} pid={} service={} roots={roots} handles={handles} waits={waits} pages={pages} pile={stack} threads={threads} cadres_ipc={ipc}",owner.number,pid.number,process.service_id));
    }

}
