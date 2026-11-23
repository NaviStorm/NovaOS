"""Témoin QEMU opt-in : consommer un ticket Direct émis avant retrait de G.
Le rendez-vous n'autorise aucun lancement : SPAWN conserve tous ses contrôles.
À installer après membership_r2 + administration_ticket_vivant + selective_exec.
"""
from pathlib import Path

def install(root):
    root=Path(root)
    def edit(name,old,new):
        p=root/name;s=p.read_text()
        if s.count(old)!=1:raise ValueError('ancrage ancien ticket '+name+': '+old[:60])
        p.write_text(s.replace(old,new))
    channel='novaOS/user/novaauth/service/src/bin/common/administration_channel.rs'
    edit(channel,'admin_command::Mutation::Disable(admin_command::Selector::Name("bob"))',
         'admin_command::Mutation::RemoveMember { user:admin_command::Selector::Name("alice"), group:admin_command::Selector::Name("base") }')
    edit(channel,'                answer = exchange(login, request.try_into().unwrap())?;',
         '                answer = exchange(login, request.try_into().unwrap())?;'+DONE)
    bob='novaOS/user/novaauth/service/administration_bob_shell.rs'
    p=root/bob;s=p.read_text();start=s.index('    let mut retired=false;');end=s.index('    login.close()?;',start)
    p.write_text(s[:start]+CONSUME+s[end:])
    session='components/shell/session_auth_lab.rs'
    edit(session,'    pub(super) fn execute(&mut self, name: &str, argv: &[&str]) -> rt::Result<u64> {',
         CONSUME_METHOD+'\n    pub(super) fn execute(&mut self, name: &str, argv: &[&str]) -> rt::Result<u64> {')
    # Fermeture normale de Bob dès son reçu et sa collecte, sans accès Alice.
    import runpy
    runpy.run_path(str(root/'novaOS/user/novaauth/service/administration_last_admin_profile.py'))['install'](root)

DONE = r'''
                if login.slot==0 && peer.slot==1 {
                    if let Ok(admin_command::Command::Confirm{operation,digest})=admin_command::decode(request) {
                        if self.prepared!=Some((operation,digest,sid)) || !self.bob_triggered
                            || answer[8..24]!=operation || answer[32]!=1 || answer[33..]!=[0;95] {
                            return Err(rt::DENIED);
                        }
                        // exchange n'a répondu COMPLETE qu'après le terminal durable.
                        let mut event=[0;128];event[..8].copy_from_slice(b"NXR2DN01");
                        event[8..40].copy_from_slice(&peer.route.client);
                        event[40..72].copy_from_slice(&login.route.client);
                        event[72..88].copy_from_slice(&operation);
                        event[88..120].copy_from_slice(&digest);
                        rt::send_for(peer.port(16),0x4e58523253544152,&event,25)?;
                    }
                }
'''
CONSUME = r'''    let done=rt::receive_for(rt::config(17),1500)?;
    if done.kind!=1 || done.tag!=event.tag || done.len!=128
        || &done.payload[..8]!=b"NXR2DN01" || done.payload[8..]!=event.payload[8..] {
        return Err(rt::DENIED);
    }
    mark(b"NAR2 Bob retrait selectif durable observe")?;
    let code=login.consume_existing(key)?;
    if code!=0 {return Err(code);}
    mark(b"NAR2 Bob ancien Direct consomme statut=0 collecte unique")?;
'''
CONSUME_METHOD = r'''    pub(super) fn consume_existing(&mut self,key:u64)->rt::Result<u64> {
        let result=(||{
            let spawned=rt::call(super::tty::SPAWN,super::tty::EXEC_TICKET,key,0);
            if spawned.status!=rt::OK {return Err(spawned.status);}
            let (pid,right)=Self::receipt(key)?;
            let rendered=self.drain(&[pid],None);
            let (fault,code)=nova_cli::process::wait(right)?;
            rendered?;
            if fault {return Err(rt::REFUSED);}
            Ok(code)
        })();
        let _=rt::call(rt::IPC_CLOSE,key,0,0);
        result
    }
'''
