"""Ordonnancement du seul témoin QEMU de ticket Bob encore vivant.

Aucun échange entre les sessions : le banc observe leurs marqueurs séparément.
Le délai ci-dessous prépare le parcours ; le ticket garde sa borne de 5 s.
"""
from pathlib import Path


def install(root):
    p = Path(root) / 'novaOS/user/novaauth/service/administration_bob_shell.rs'
    s = p.read_text()
    anchor = '    login.close()?;'
    if s.count(anchor) != 1:
        raise ValueError('ancrage du témoin Bob absent ou ambigu')
    s = s.replace(anchor, '''    mark(b"NAR2 Bob attente preparation Alice ticket non emis")?;
    // Alice peut préparer sa mutation pendant cette attente, sans aucun accès
    // à la route de Bob. Ce délai ne prolonge ni session, ni défi, ni ticket.
    // IPC_RECEIVE attend des ticks ; HZ=250 dans le noyau qualifié.
    match rt::receive_for(rt::config(1),15*250) {
        Err(rt::TIMED_OUT)=>(),
        _=>return Err(rt::REFUSED),
    }
    mark(b"NAR2 Bob preparation terminee emission ticket")?;
    let key=login.ticket("admin-sonde", &["--self"],5000,1,0)?;
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
    p.write_text(s.replace(anchor, anchor.replace('fn receipt', 'pub(super) fn receipt')))
