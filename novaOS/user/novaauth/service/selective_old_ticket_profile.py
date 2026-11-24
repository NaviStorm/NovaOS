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
    import runpy
    rendezvous = runpy.run_path(str(root/Path(channel).parents[3]/'administration_rendezvous_profile.py'))
    edit(channel, rendezvous['PENDING'], RESUME.replace('TERMINAL_EVENT', DONE))
    trigger = rendezvous['TRIGGER'].replace(
        'admin_command::Mutation::Disable(admin_command::Selector::Name("bob"))',
        'admin_command::Mutation::RemoveMember { user:admin_command::Selector::Name("bob"), group:admin_command::Selector::Name("base") }',
    )
    start = trigger.index('                        // La seconde préparation')
    end = trigger.index('                    } else {', start)
    edit(channel, rendezvous['TRIGGER'], trigger[:start]+trigger[end:])

    # Reprendre la même vérification de capacité et de session au moment où
    # la confirmation est réellement envoyée, après les échanges de Bob.
    path = root/channel
    source = path.read_text()
    start = source.index('                let mut meta = [0; 31];')
    end = source.index('                #[cfg(feature="administration-r2-guest")]\n                if matches!', start)
    context = source[start:end]
    source = source[:start] + '                let (sid, deadline) = confirm_context(login,key,request,None)?;\n' + source[end:]
    helper = ('\nfn confirm_context(login:&mut Login,key:u64,request:&[u8],expected:Option<[u8;32]>)'
              ' -> rt::Result<([u8;32],u64)> {\n'
              + ''.join(line[12:] if line.startswith(' '*12) else line for line in context.splitlines(keepends=True))
              + '    Ok((sid,meta[29]))\n}\n')
    anchor = '    let sid = bytes::<32>(&meta[5..9]);'
    assert helper.count(anchor)==1
    helper = helper.replace(anchor, anchor+'\n    if expected.is_some_and(|id| id != sid) {return Err(rt::DENIED);}')
    path.write_text(source+helper)
    edit(channel,
        '                        if exchange(login,&pre)?!=selected {return Err(rt::DENIED);}',
        '                        if exchange(login,&pre)?!=selected {return Err(rt::DENIED);}'+AFTER_EFFECTORS)
    verifier = 'novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
    edit(verifier, '        self.r2_offered=true;\n        Ok(())',
         '        self.r2_offered=true;\n        if self.slot==1 {self.r2_ready=Some(id);}\n        Ok(())')
    bob='novaOS/user/novaauth/service/administration_bob_shell.rs'
    # L'offre est déjà acquittée par le noyau pour la session inspectée.
    # Utiliser cet événement existant pour le rendez-vous de qualification.
    edit(bob, '    login.authenticate()?; // session existante : NSHLIVE seulement, aucun nouveau bail\n', '')
    p=root/bob;s=p.read_text();start=s.index('    let mut retired=false;');end=s.index('    login.close()?;',start)
    p.write_text(s[:start]+CONSUME+s[end:])
    session='components/shell/session_auth_lab.rs'
    edit(session,'    pub(super) fn execute(&mut self, name: &str, argv: &[&str]) -> rt::Result<u64> {',
         CONSUME_METHOD+'\n    pub(super) fn execute(&mut self, name: &str, argv: &[&str]) -> rt::Result<u64> {')
    # Fermeture normale de Bob dès son reçu et sa collecte, sans accès Alice.
    runpy.run_path(str(root/'novaOS/user/novaauth/service/administration_last_admin_profile.py'))['install'](root)

AFTER_EFFECTORS = r'''
                        if selective && login.slot==0 && peer.slot==1
                            && let Ok(admin_command::Command::Confirm{operation,digest})=
                                admin_command::decode(request.try_into().unwrap())
                            && self.prepared==Some((operation,digest,sid)) {
                            if self.bob_triggered || peer.live.is_none() {return Err(rt::DENIED);}
                            let until=now()?.saturating_add(20_000).min(deadline);
                            let mut event=[0;128];
                            event[..8].copy_from_slice(b"NXR2GO01");
                            event[8..40].copy_from_slice(&peer.route.client);
                            event[40..72].copy_from_slice(&login.route.client);
                            event[72..88].copy_from_slice(&operation);
                            event[88..120].copy_from_slice(&digest);
                            peer.r2_ready=None;
                            peer.r2_offered=false;
                            rt::send_for(peer.port(16),0x4e58523253544152,&event,25)?;
                            self.bob_triggered=true;
                            // Même emplacement borné que l'ancienne réponse
                            // différée. La confirmation n'a pas encore été envoyée.
                            answer.copy_from_slice(request);
                            defer=Some((sid,until));
                            return Ok(());
                        }
'''
RESUME = r'''            if let Some((tag, request, sid, until))=lane.pending {
                let tick=now().unwrap_or(u64::MAX);
                let ready=peer.r2_ready.is_some_and(|id|peer.live.is_some_and(|(active,_)|active==id));
                if !ready && tick<until {continue;}
                lane.pending=None;
                let mut answer=[0;128];
                let result=(||{
                    if tick>=until {return Err(rt::TIMED_OUT);}
                    confirm_context(login,key,&request,Some(sid))?;
                    r2_probe(21);
                    answer=exchange(login,&request)?;
                    let request=&request;
TERMINAL_EVENT
                    r2_probe(22);
                    Ok(())
                })();
                if let Err(code)=result{
                    answer.fill(0);
                    answer[..8].copy_from_slice(&code.to_le_bytes());
                }
                send_answer(key,tag,&answer);
                continue;
            }'''
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
