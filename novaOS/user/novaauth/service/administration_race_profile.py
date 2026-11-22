"""S7 : présenter le ticket dès le rendez-vous, pendant la confirmation Alice.

Variante du témoin vivant, mêmes capacités/délais. Un succès admis avant le retrait
reste un résultat légitime à collecter ; un refus n'est jamais converti en succès.
"""
from pathlib import Path


def install(root):
    root=Path(root);p=root/'novaOS/user/novaauth/service/administration_bob_shell.rs';s=p.read_text()
    old='login.ticket("admin-sonde", &["--self"],5000,1,0)?'
    if s.count(old)!=1:raise ValueError('ancrage ticket témoin ordinaire')
    s=s.replace(old,'login.ticket("temoin", &[],5000,1,0)?')
    begin=s.index('    let mut retired=false;');end=s.index('    login.close()?;',begin)
    s=s[:begin]+RACE+s[end:];p.write_text(s)
    p=root/'components/shell/session_auth_lab.rs';s=p.read_text();old='    fn drain(&mut self,'
    if s.count(old)!=1:raise ValueError('ancrage drainage course R2')
    p.write_text(s.replace(old,'    pub(super) fn drain_owned(&mut self,pid:u64)->rt::Result<()> {self.drain(&[pid],None)}\n'+old))

    p=root/'novaOS/user/novaauth/service/src/bin/common/administration_channel.rs';s=p.read_text()
    anchor='                    let mut pre:[u8;128]=request.try_into().unwrap();'
    if s.count(anchor)!=1:raise ValueError('ancrage confirmation corrélée')
    p.write_text(s.replace(anchor,RUN+anchor))

RUN=r'''
                    // Rendez-vous du banc seulement, après inspection de la
                    // session/capacité, avant le retrait durable existant.
                    let (operation,digest)=match admin_command::decode(request) {
                        Ok(admin_command::Command::Confirm{operation,digest})=>(operation,digest),
                        _=>return Err(rt::DENIED),
                    };
                    if self.prepared.take()!=Some((operation,digest,sid)) || !self.bob_triggered
                        || login.slot!=0 || peer.slot!=1 {return Err(rt::DENIED);}
                    let mut event=[0;128];event[..8].copy_from_slice(b"NXR2RN01");
                    event[8..40].copy_from_slice(&peer.route.client);
                    event[40..72].copy_from_slice(&login.route.client);
                    event[72..88].copy_from_slice(&operation);
                    event[88..120].copy_from_slice(&digest);
                    rt::send_for(peer.port(16),0x4e58523253544152,&event,25)?;
'''

RACE=r'''
    let run=rt::receive_for(rt::config(17),1250)?;
    if run.kind!=1 || run.tag!=issue.tag || run.len!=128
        || &run.payload[..8]!=b"NXR2RN01" || run.payload[8..]!=issue.payload[8..] {
        return Err(rt::DENIED);
    }
    probe(31,0,0,0);
    let launch=rt::call(super::tty::SPAWN,super::tty::EXEC_TICKET,key,0);
    probe(32,0,1,launch.status);
    if launch.status==0 {
        let (pid,right)=session_auth_lab::Login::receipt(key)?;
        if right!=launch.value || pid==0 {return Err(rt::REFUSED);}
        let rendered=login.drain_owned(pid);
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
