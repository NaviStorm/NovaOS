//! Témoin inclus uniquement par l'export QEMU exec-admission.
use super::rt;
use core::sync::atomic::{AtomicU64, Ordering::SeqCst};
struct Worker<'a> {
    key:u64, go:&'a AtomicU64, ready:&'a AtomicU64,
    id:AtomicU64, status:AtomicU64, child:AtomicU64, legacy:AtomicU64,
}
extern "C" fn entry(argument:u64)->! {
    let w=unsafe {&*(argument as *const Worker<'_>)};
    w.id.store(rt::call(rt::THREAD_ID,0,0,0).value,SeqCst);
    let mut refused=0;
    for shape in [0,1,2] {
        if rt::call(super::super::tty::SPAWN,shape,0,0).status==rt::DENIED {refused+=1;}
    }
    w.legacy.store(refused,SeqCst);w.ready.fetch_add(1,SeqCst);
    while w.go.load(SeqCst)==0 {rt::call(rt::THREAD_YIELD,0,0,0);}
    if w.go.load(SeqCst)==1 {
        let result=rt::call(super::super::tty::SPAWN,super::super::tty::EXEC_TICKET,w.key,0);
        w.child.store(result.value,SeqCst);w.status.store(result.status,SeqCst);
    }
    rt::call(rt::THREAD_EXIT,0,0,0);
    loop {core::hint::spin_loop();}
}
pub(super) fn consume(key:u64)->rt::Result<rt::Reply> {
    // Les opérations de détachement, annulation, TLS et futex ne sont pas accordées.
    for id in [rt::THREAD_DETACH,rt::THREAD_CANCEL,rt::THREAD_CANCELLED,rt::THREAD_TLS,rt::THREAD_WAIT,rt::THREAD_NOTIFY] {
        if rt::call(id,0,0,0).status!=rt::DENIED {return Err(rt::REFUSED);}
    }
    let ready=AtomicU64::new(0);let go=AtomicU64::new(0);
    let workers:[Worker<'_>;2]=core::array::from_fn(|_|Worker {
        key,go:&go,ready:&ready,id:AtomicU64::new(0),status:AtomicU64::new(u64::MAX),child:AtomicU64::new(0),legacy:AtomicU64::new(0)});
    let mut rights=[0;2];let mut failure=None;
    for i in 0..2 {
        let result=rt::call(rt::THREAD_CREATE,entry as *const () as u64,&workers[i] as *const _ as u64,4);
        if result.status!=0 {failure=Some(result.status);break;}rights[i]=result.value;
    }
    if failure.is_none() {
        for _ in 0..10000 {
            if ready.load(SeqCst)==2 {break;}rt::call(rt::THREAD_YIELD,0,0,0);
        }
        if ready.load(SeqCst)!=2 {failure=Some(rt::TIMED_OUT);}
    }
    go.store(if failure.is_some(){2}else{1},SeqCst);
    for right in rights.into_iter().filter(|r|*r!=0) {
        let r=rt::call(rt::THREAD_JOIN,right,0,0);
        if r.status!=0 {failure=Some(r.status);}
        if rt::call(rt::THREAD_JOIN,right,0,0).status!=rt::DENIED {failure=Some(rt::REFUSED);}
    }
    if let Some(e)=failure {return Err(e);}
    let mut result=rt::Reply {status:rt::REFUSED,value:0};let mut successes=0;let mut refusals=0;
    for w in &workers {
        let (id,status,child,legacy)=(w.id.load(SeqCst),w.status.load(SeqCst),w.child.load(SeqCst),w.legacy.load(SeqCst));
        super::super::console::write(super::super::tty::OUTPUT,&alloc::format!(
            "NXEXEC concurrence ticket={key} thread={id} statut={status} droit_collecte={child} anciens_spawns_refuses={legacy}/3\n"))?;
        if legacy!=3 {return Err(rt::REFUSED);}
        if status==0 {successes+=1;result=rt::Reply {status,value:child};}
        else if status==rt::REFUSED {refusals+=1;}
    }
    if successes!=1 || refusals!=1 || workers[0].id.load(SeqCst)==workers[1].id.load(SeqCst) {return Err(rt::REFUSED);}
    Ok(result)
}

// La fermeture WOULD_BLOCK n'a aucun effet : attendre l'issue distante déjà
// engagée. Ne jamais réémettre SPAWN ou une mutation pour libérer un reçu.
pub(super) fn close_ticket(key:u64)->rt::Result<()> {
    for _ in 0..1500 {
        let status=rt::call(rt::IPC_CLOSE,key,0,0).status;
        if status==0 {return Ok(());}
        if status!=rt::WOULD_BLOCK {return Err(status);}
        match rt::receive_for(rt::config(17),2) {
            Err(rt::TIMED_OUT|rt::WOULD_BLOCK)=>(),
            other=>return Err(other.err().unwrap_or(rt::REFUSED)),
        }
    }
    Err(22) // UNCERTAIN : garder le reçu, ne jamais prétendre le drainage acquis.
}
