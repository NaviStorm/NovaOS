"""Recette S9 : réactivation explicite, sans restauration des anciens droits."""
from pathlib import Path


def install(root):
    root=Path(root)
    def edit(name,old,new):
        p=root/name;s=p.read_text()
        if s.count(old)!=1:raise ValueError('ancrage S9 absent/ambigu : '+name+' '+old[:60])
        p.write_text(s.replace(old,new))
    bob='novaOS/user/novaauth/service/administration_bob_shell.rs'
    edit(bob,'    let key=login.ticket(', '    let old_session=login.session.ok_or(rt::DENIED)?;\n    let key=login.ticket(')
    edit(bob,'    rt::value(rt::call(rt::IPC_CLOSE,key,0,0))?;\n    login.close()?;',REAUTH)
    login='components/shell/session_auth_lab.rs'
    edit(login,'    session: Option<[u8; 32]>,','    pub(super) session: Option<[u8; 32]>,')
    edit(login,'    fn request(&mut self, op:', '    pub(super) fn request(&mut self, op:')
    channel='novaOS/user/novaauth/service/src/bin/common/administration_channel.rs'
    edit(channel,'    bob_triggered: bool,','    bob_triggered: bool,\n    reactivation: Option<[u8;16]>,')
    edit(channel,'            bob_triggered: false,','            bob_triggered: false,\n            reactivation: None,')
    anchor='                answer = exchange(login, request.try_into().unwrap())?;'
    edit(channel,anchor,anchor+TRIGGER)

REAUTH=r'''    login.close()?;
    mark(b"NAR2 Bob attente reactivation explicite")?;
    let event=rt::receive_for(rt::config(17),15000)?;
    if event.kind!=1 || event.tag!=0x4e58523252454143 || event.len!=128
        || &event.payload[..8]!=b"NXR2RE01" || event.payload[8..40]!=target
        || event.payload[40..72]==[0;32] || event.payload[72..88]==[0;16]
        || event.payload[88..]!=[0;40] {return Err(rt::DENIED);}
    login.authenticate()?;
    if login.identity!=target || login.session==Some(old_session) {return Err(rt::DENIED);}
    mark(b"NAR2 Bob nouvelle session explicite distincte")?;
    if rt::call(super::tty::SPAWN,super::tty::EXEC_TICKET,key,0).status!=rt::REVOKED {
        return Err(rt::REFUSED);
    }
    if login.request(b"NSHLIVE1",&old_session).err()!=Some(rt::DENIED) {return Err(rt::REFUSED);}
    mark(b"NAR2 Bob apres reactivation ancien ticket=10 ancienne session=5")?;
    rt::value(rt::call(rt::IPC_CLOSE,key,0,0))?;
    login.authenticate()?; // inspecte la nouvelle session, aucun renouvellement
    if login.execute("admin-sonde", &["--self"])?!=0 {return Err(rt::REFUSED);}
    mark(b"NAR2 Bob nouveau lancement autorise droits ordinaires")?;
    login.close()?;'''

TRIGGER=r'''
                if login.slot==0 && peer.slot==1 {
                    match admin_command::decode(request) {
                        Ok(admin_command::Command::Prepare(admin_command::Mutation::Reactivate(admin_command::Selector::Name("bob"))))=> {
                            if answer[8..24]==[0;16] {return Err(rt::DENIED);}
                            self.reactivation=Some(answer[8..24].try_into().unwrap());
                        },
                        Ok(admin_command::Command::Confirm{operation,..}) if self.reactivation==Some(operation)=> {
                            if answer[8..24]!=operation || answer[32]!=1 || peer.live.is_some() {return Err(rt::DENIED);}
                            self.reactivation=None;
                            let mut event=[0;128];event[..8].copy_from_slice(b"NXR2RE01");
                            event[8..40].copy_from_slice(&peer.route.client);
                            event[40..72].copy_from_slice(&login.route.client);
                            event[72..88].copy_from_slice(&operation);
                            rt::send_for(peer.port(16),0x4e58523252454143,&event,25)?;
                        },
                        _=>(),
                    }
                }
'''
