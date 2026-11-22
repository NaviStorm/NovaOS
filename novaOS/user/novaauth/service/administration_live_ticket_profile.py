"""Témoin QEMU séparé : Bob s'authentifie sur un événement privé unique.
Les parcours négatifs ordinaires restent intacts, sans nouvelle racine IPC.
"""
from pathlib import Path


def install(root):
    p = Path(root) / 'novaOS/user/novaauth/service/administration_bob_shell.rs'
    s = p.read_text()
    start = s.index('    let mut login=')
    end = s.index('    login.close()?;', start)
    s = s[:start] + INITIAL + s[end:]
    anchor = '    login.close()?;'
    if s.count(anchor) != 1:
        raise ValueError('ancrage du témoin Bob absent ou ambigu')
    s = s.replace(anchor, '''    let key=login.ticket("admin-sonde", &["--self"],5000,1,0)?;
    mark(b"NAR2 Bob ticket vivant non consomme")?;
    let mut retired=false;
    // Observer au plus 6 s ; l'oracle externe exige toujours maintenant <
    // échéance du ticket de 5 s. Ce délai de diagnostic ne rend aucun droit.
    for _ in 0..300 {
        // Reçu local existant, en lecture seule : aucun appel à l'autorité,
        // aucune consommation et aucun lancement pendant cette observation.
        match session_auth_lab::Login::receipt(key) {
            Err(rt::REVOKED)=>{mark(b"NAR2 Bob recu local revoque statut=10")?;retired=true;break;},
            Err(rt::WOULD_BLOCK)=>(),
            _=>return Err(rt::REFUSED),
        }
        match rt::receive_for(rt::config(1),5) {
            Err(rt::TIMED_OUT)=>(),
            _=>return Err(rt::REFUSED),
        }
    }
    if !retired {return Err(rt::REFUSED);}
    let answer=rt::call(super::tty::SPAWN,super::tty::EXEC_TICKET,key,0);
    if answer.status!=rt::REVOKED {return Err(rt::REFUSED);}
    mark(b"NAR2 Bob ancien ticket statut=10 aucun enfant")?;
    rt::value(rt::call(rt::IPC_CLOSE,key,0,0))?;
    login.close()?;''')
    p.write_text(s)
    p = Path(root) / 'components/shell/session_auth_lab.rs'
    s = p.read_text()
    anchor = '    fn receipt(key: u64) -> rt::Result<(u64, u64)> {'
    if s.count(anchor) != 1:
        raise ValueError('ancrage consultation locale du reçu absent ou ambigu')
    s = s.replace('    identity: [u8; 32],', '    pub(super) identity: [u8; 32],')
    p.write_text(s.replace(anchor, anchor.replace('fn receipt', 'pub(super) fn receipt')))

    p = Path(root) / 'novaOS/user/novaauth/service/src/bin/common/administration_channel.rs'
    s = p.read_text()
    s = s.replace('    lanes: [Lane; 2],', '    lanes: [Lane; 2],\n    bob_triggered: bool,')
    s = s.replace('            lanes: core::array', '            bob_triggered: false,\n            lanes: core::array')
    anchor = '                answer = exchange(login, request.try_into().unwrap())?;'
    if s.count(anchor) != 1:
        raise ValueError('ancrage réponse préparée absent ou ambigu')
    p.write_text(s.replace(anchor, anchor + TRIGGER))

INITIAL = r'''
    mark(b"NAR2 Bob attente declenchement authentification")?;
    // Canal privé préexistant du vérificateur, sans droit nouveau. Ce message
    // ordonne uniquement le banc : il ne constitue jamais une authentification.
    let event=rt::receive_for(rt::config(17),15000)?;
    if event.kind!=1 || event.tag!=0x4e58523253544152 || event.len!=128
        || &event.payload[..8]!=b"NXR2GO01" || event.payload[120..]!=[0;8]
        || event.payload[8..40]==[0;32] || event.payload[40..72]==[0;32]
        || event.payload[72..88]==[0;16] || event.payload[88..120]==[0;32] {
        return Err(rt::DENIED);
    }
    let target:[u8;32]=event.payload[8..40].try_into().unwrap();
    let mut label=*b"NAR2 Bob declenchement operation=00000000000000000000000000000000";
    for (i,b) in event.payload[72..88].iter().enumerate() {
        label[33+2*i]=b"0123456789abcdef"[(b>>4) as usize];
        label[34+2*i]=b"0123456789abcdef"[(b&15) as usize];
    }
    mark(&label)?;
    let mut login=session_auth_lab::Login::default();
    login.authenticate()?;
    if login.identity!=target {return Err(rt::DENIED);}
    mark(b"NAR2 Bob authentifie route distincte")?;
    mark(b"NAR2 Bob preparation terminee emission ticket")?;
'''

TRIGGER = r'''

                // Une seule préparation réussie d'Alice déclenche Bob. Aucun
                // octroi de droit : sa vraie authentification suit sur sa route.
                if !self.bob_triggered && login.slot==0 && peer.slot==1
                    && matches!(admin_command::decode(request),
                        Ok(admin_command::Command::Prepare(admin_command::Mutation::Disable(admin_command::Selector::Name("bob"))))) {
                    if peer.live.is_some() || answer[8..24]==[0;16]
                        || answer[24..56]==[0;32] {return Err(rt::DENIED);}
                    let mut event=[0;128];event[..8].copy_from_slice(b"NXR2GO01");
                    event[8..40].copy_from_slice(&peer.route.client);
                    event[40..72].copy_from_slice(&login.route.client);
                    event[72..88].copy_from_slice(&answer[8..24]);
                    event[88..120].copy_from_slice(&answer[24..56]);
                    self.bob_triggered=true;
                    rt::send_for(peer.port(16),0x4e58523253544152,&event,25)?;
                }
'''
