//! Sonde de rejeu, injectée uniquement dans le nsh du profil QEMU exec-admission.
use super::{console,rt,tty};
use core::sync::atomic::{AtomicBool,Ordering};
static CLOSED:AtomicBool=AtomicBool::new(false);

// La première saisie ferme irréversiblement la fenêtre, y compris si elle est
// invalide ou demande une authentification qui échoue. Aucun ticket n'est créé.
pub(super) fn intercept(source:&str)->rt::Result<Option<u64>> {
    let available=!CLOSED.swap(true,Ordering::SeqCst);
    let mut words=source.split_ascii_whitespace();
    if words.next()!=Some("ticket-reference") {return Ok(None);}
    if !available {
        console::write(tty::OUTPUT,"NXREFERENCE sonde fermee aucun appel SPAWN\n")?;
        return Ok(Some(rt::DENIED));
    }
    let raw=words.next().ok_or(rt::BAD_ARGUMENT)?;
    if raw.is_empty() || !raw.bytes().all(|b|b.is_ascii_digit()) || words.next().is_some() {
        return Err(rt::BAD_ARGUMENT);
    }
    let key=raw.parse::<u64>().map_err(|_|rt::BAD_ARGUMENT)?;
    let reply=rt::call(tty::SPAWN,tty::EXEC_TICKET,key,0);
    console::write(tty::OUTPUT,&alloc::format!(
        "NXREFERENCE ticket={key} statut={} valeur={} appels=1\n",reply.status,reply.value))?;
    // Un succès demeure un succès natif observable : le banc doit échouer.
    // Aucun retry, aucune fermeture ou nouvelle capacité ne masque ce résultat.
    Ok(Some(reply.status))
}
