"""Témoin QEMU séparé : Bob s'authentifie puis attend un événement privé corrélé au ticket.
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
    import runpy
    runpy.run_path(str(Path(root) / 'novaOS/user/novaauth/service/administration_refusal_trace.py'))['install'](root)
    runpy.run_path(str(Path(root) / 'novaOS/user/novaauth/service/administration_rendezvous_profile.py'))['install'](root)
    runpy.run_path(str(Path(root) / 'novaOS/user/novaauth/service/administration_status_profile.py'))['install'](root)
    runpy.run_path(str(Path(root) / 'novaOS/user/novaauth/service/administration_transport_512_profile.py'))['install'](root)

INITIAL = r'''
    mark(b"NAR2 Bob attente declenchement authentification")?;
    // Canal privé préexistant du vérificateur, sans droit nouveau. Ce message
    // ordonne uniquement le banc : il ne constitue jamais une authentification.
    let event=rt::receive_for(rt::config(17),15000)?;
    if event.kind!=1 || event.tag!=0x4e58523253544152 || event.len!=128
        || &event.payload[..8]!=b"NXR2AU01" || event.payload[120..]!=[0;8]
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
    // Deuxième étape du même scénario : ne charger/émettre aucun ticket avant
    // que l'ELF de confirmation d'Alice soit effectivement résident.
    let issue=rt::receive_for(rt::config(17),15000)?;
    if issue.kind!=1 || issue.tag!=event.tag || issue.len!=128
        || &issue.payload[..8]!=b"NXR2GO01" || issue.payload[8..]!=event.payload[8..] {
        return Err(rt::DENIED);
    }
    mark(b"NAR2 Bob preparation terminee emission ticket")?;
'''

TRIGGER = ""  # Le rendez-vous est installé après les sondes historiques.
