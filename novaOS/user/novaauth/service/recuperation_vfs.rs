//! Effecteur de lecture : inclus seulement par le profil de récupération QEMU.
//! Aucun reçu historique ni génération locale ne vaut autorisation.
use super::*;

fn number(b:&[u8])->u64 {u64::from_le_bytes(b[..8].try_into().unwrap())}
fn port(lane:usize)->u64 {
    unsafe {(&*(0x401000 as *const core::sync::atomic::AtomicU64).add(24+2*lane))
        .load(core::sync::atomic::Ordering::Acquire)}
}
pub(super) fn read(session:&mut Session<'_, '_>,a:&Frame)->rt::Result<()> {
    if &a[..8]!=b"NA5READ1" || a[8]>1 || a[9..16]!=[0;7] ||
       a[112..]!=[0;16] || (a[40..72]==[0;32] && a[72..104]!=[0;32]) {return Err(rt::DENIED);}
    let channel=port(a[8] as usize);
    if channel==0 {return Err(rt::DENIED);}
    let tag=number(&a[16..]);
    // Consommer uniquement la demande déjà émise par le client authentifié.
    let mut selected=None;
    for _ in 0..16 {
        let msg=rt::receive_for(channel,0)?;
        if msg.tag<tag {continue;}
        if msg.kind!=1 || msg.len!=128 || msg.tag!=tag {return Err(rt::DENIED);}
        selected=Some(msg);break;
    }
    let msg=selected.ok_or(rt::DENIED)?;
    let mut reply=[0;128];
    let result=(|| {
        if a[72..104]==[0;32] {return Err(rt::DENIED);}
        if session.failed.is_some() || session.pending.is_some() {return Err(rt::REFUSED);}
        let q=&msg.payload;
        if q[0]!=12 || q[1..8]!=[0;7] || q[24..]!=[0;104] ||
           q[8..16]!=a[104..112] {return Err(rt::DENIED);}
        session.mount_volume()?;
        let fs=session.fs.as_mut().ok_or(rt::CLOSED)?;
        // Deux objets du banc, non accessibles depuis le namespace public.
        // L'OID seul n'autorise rien : sceau lié au titulaire par le registre.
        let object=number(&q[8..]);
        let root=fs.lookup_at(fs.root_id(),"\u{1f}NovaAuth-D").map_err(error)?;
        let dir=["P","L"].into_iter().find_map(|name|fs.lookup_at(root.id,name).ok()
            .filter(|e| e.id==object && e.kind==Kind::Directory)).ok_or(rt::DENIED)?;
        let policy=fs.lookup_at(dir.id,"politique").map_err(error)?;
        if policy.kind!=Kind::File || policy.size!=104 {return Err(rt::DENIED);}
        let seal=fs.inspect_id(policy.id,0,104).map_err(error)?;
        let digest:[u8;32]=novaos_identity::recuperation_view::seal_digest(&seal);
        if digest!=a[72..104] || &seal[..8]!=b"NADIR001" ||
           seal[8..24]!=a[24..40] || seal[24..40]!=fs.ids().fs ||
           seal[40..56]!=fs.ids().lv || number(&seal[56..])!=dir.id ||
           number(&seal[88..])!=policy.id || seal[96]&1==0 || seal[96]&!3!=0 ||
           seal[97]!=1 || seal[98..]!=[0;6] {return Err(rt::DENIED);}
        // Aucun chemin dans le sous-arbre surprotégé n'est admis par ce port.
        let file=fs.lookup_at(dir.id,"registre").map_err(error)?;
        if file.kind!=Kind::File || file.id!=number(&seal[64..]) {return Err(rt::DENIED);}
        let offset=number(&q[16..]);
        if offset.checked_add(8).is_none_or(|end|end>file.size) {return Err(rt::BAD_ARGUMENT);}
        let bytes=fs.read_access_id(file.id,offset,8).map_err(error)?;
        if bytes.len()!=8 {return Err(rt::REFUSED);}
        reply[8..16].copy_from_slice(&8u64.to_le_bytes());
        reply[16..24].copy_from_slice(&bytes);
        Ok(())
    })();
    if let Err(code)=result {reply=[0;128];reply[..8].copy_from_slice(&code.to_le_bytes());}
    reply[120..].copy_from_slice(b"DIRREPLY");
    let label=if number(&reply)==0 {b"NARV VFS lecture rc=0 octets=8".as_slice()}
        else {b"NARV VFS refus octets=0".as_slice()};
    rt::call(rt::DEBUG_WRITE,label.as_ptr() as u64,label.len() as u64,0);
    // Une seule réponse bornée, comme le port de dossier historique ; le client
    // attend l'acquittement du vérificateur avant de collecter ces octets.
    rt::send_frame(channel,tag,&reply,0,false).map_err(|_|w::UNCERTAIN)?;
    Ok(())
}
