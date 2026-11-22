"""S7 : présenter le ticket dès le rendez-vous, pendant la confirmation Alice.

Variante du témoin vivant, mêmes capacités/délais. Un succès admis avant le retrait
reste un résultat légitime à collecter ; un refus n'est jamais converti en succès.
"""
from pathlib import Path


def install(root):
    root=Path(root);p=root/'novaOS/user/novaauth/service/administration_bob_shell.rs';s=p.read_text()
    begin=s.index('    let mut retired=false;');end=s.index('    login.close()?;',begin)
    s=s[:begin]+RACE+s[end:];p.write_text(s)
    p=root/'components/shell/session_auth_lab.rs';s=p.read_text();old='    fn drain(&mut self,'
    if s.count(old)!=1:raise ValueError('ancrage drainage course R2')
    p.write_text(s.replace(old,'    pub(super) fn drain(&mut self,'))

RACE=r'''
    probe(31,0,0,0);
    let launch=rt::call(super::tty::SPAWN,super::tty::EXEC_TICKET,key,0);
    probe(32,0,1,launch.status);
    if launch.status==0 {
        let (pid,right)=session_auth_lab::Login::receipt(key)?;
        if right!=launch.value || pid==0 {return Err(rt::REFUSED);}
        let rendered=login.drain(&[pid],None);
        let (fault,status)=nova_cli::process::wait(right)?;
        if fault || !matches!(status,0|rt::DENIED|rt::REVOKED) {return Err(rt::REFUSED);}
        if let Err(code)=rendered {if !matches!(code,rt::DENIED|rt::REVOKED|rt::CLOSED) {return Err(code);}}
        mark(alloc::format!("NAR2 course enfant={pid} statut={status} collecte unique").as_bytes())?;
    } else if launch.status==rt::REVOKED {
        mark(b"NAR2 course refus=10 aucun enfant")?;
    } else {return Err(launch.status);}
    // Attendre uniquement l'issue de la fermeture d'un reçu déjà engagé.
    // Jamais de second SPAWN ou de mutation pour obtenir son acquittement.
    let mut closed=false;
    for _ in 0..1500 {
        match rt::call(rt::IPC_CLOSE,key,0,0).status {
            0=>{closed=true;break;},
            rt::WOULD_BLOCK=>match rt::receive_for(rt::config(1),2) {Err(rt::TIMED_OUT)=>(),_=>return Err(rt::REFUSED)},
            code=>return Err(code),
        }
    }
    if !closed {return Err(22);}
    mark(b"NAR2 course issue collectee")?;
'''
