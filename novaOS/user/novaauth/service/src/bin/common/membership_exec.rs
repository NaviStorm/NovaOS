//! Inspection privée de l'effecteur EXEC sous le verrou d'admission du noyau.
//! Pas d'ACK sur une route interrompue ou une admission dont l'origine manque.
use super::*;
pub(super) fn independent(login:&mut Login, subject:[u8;16], group:[u8;16], revision:u64,
    operation:[u8;16])->rt::Result<()> {
    let (_,_,sid)=login.retirement_binding.filter(|(u,_,sid)|*u==subject
        && login.live.is_some_and(|(active,_)|active==*sid)).ok_or(rt::REVOKED)?;
    if group==[0;16] || revision>=1<<63 || operation==[0;16] {return Err(rt::BAD_ARGUMENT);}
    login.bind_relay(sid)?;
    login.relay_seq=login.relay_seq.checked_add(1).ok_or(rt::LIMIT)?;
    let mut q=[0;128];q[..8].copy_from_slice(b"NXMGEXE1");
    q[8..40].copy_from_slice(&sid);q[40..72].copy_from_slice(&login.route.client);
    q[72..88].copy_from_slice(&subject);q[88..104].copy_from_slice(&group);
    q[104..112].copy_from_slice(&revision.to_le_bytes());q[112..].copy_from_slice(&operation);
    // Ce SEND est intercepté par le noyau sur la route privée du vérificateur,
    // après contrôle du propriétaire, de l'instance, de la session et du tag.
    rt::send_for(login.port(19),login.relay_seq,&q,250)
}
