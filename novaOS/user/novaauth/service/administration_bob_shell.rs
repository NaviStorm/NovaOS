//! Parcours de qualification dans le même ELF nsh, sans console ni VFS général.
//! Le collecteur des octets reste exclusivement dans le banc hôte.
use super::{rt, session_auth_lab};
fn mark(message:&[u8])->rt::Result<()> {
    rt::value(rt::call(rt::DEBUG_WRITE,message.as_ptr() as u64,message.len() as u64,0)).map(|_|())
}
pub(super) fn session()->rt::Result<()> {
    while rt::config(16)==0 || rt::config(17)==0 {
        match rt::receive_for(rt::config(1),2) {
            Err(rt::TIMED_OUT|rt::WOULD_BLOCK)=>(),
            _=>return Err(rt::CANCELLED),
        }
    }
    let mut login=session_auth_lab::Login::default();
    login.authenticate()?;
    mark(b"NAR2 Bob authentifie route distincte")?;
    // Même chemin de résolution/admission que les lancements autorisés. Le
    // premier paquet existe exclusivement dans le catalogue d'Alice ; le
    // second n'est autorisé à aucun des deux titulaires.
    for name in ["alice-seulement", "paquet-non-autorise"] {
        if !matches!(login.execute(name, &[]), Err(rt::DENIED)) {
            return Err(rt::REFUSED);
        }
    }
    mark(b"NAR2 Bob paquets Alice et non autorise refuses statut=5 aucun enfant")?;
    let status=login.execute("admin-sonde", &["--r2-refus"])?;
    if status!=0 {return Err(status);}
    mark(b"NAR2 Bob consultation propre terminee")?;
    mark(b"NAR2 Bob refus serveur et substitutions verifies")?;
    login.close()?;
    mark(b"NAR2 Bob fermeture acquittee")?;
    loop {
        match rt::receive_for(rt::config(1),25) {
            Err(rt::TIMED_OUT|rt::WOULD_BLOCK)=>(),
            Ok(m) if m.kind==1 && m.len==1 && m.payload[0]==0=>break,
            _=>return Err(rt::DENIED),
        }
    }
    Ok(())
}
