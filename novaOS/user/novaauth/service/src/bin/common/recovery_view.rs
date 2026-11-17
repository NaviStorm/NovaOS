//! Le vérificateur obtient seulement une liaison de facteur sur une route
//! authentifiée par le noyau. La connaissance d'une référence ne crée pas une
//! session : la preuve cryptographique reste exigée dans le vérificateur.
use crate::{common,recovery_guest::Activated,runtime as rt};
use novaos_identity::{Head,recuperation_view::View};
fn head(b:&[u8;144])->Head {
    Head {domain:b[..16].try_into().unwrap(),
        volume:[b[16..32].try_into().unwrap(),b[32..48].try_into().unwrap()],
        recovery:b[48..64].try_into().unwrap(),
        objects:core::array::from_fn(|i|u64::from_be_bytes(b[64+8*i..72+8*i].try_into().unwrap())),
        generation:u64::from_be_bytes(b[88..96].try_into().unwrap()),
        request:b[96..112].try_into().unwrap(),digest:b[112..].try_into().unwrap()}
}
pub fn serve(mut recovered:Activated)->rt::Result<()> {
    let mut state=View::read(head(&recovered.head),recovered.lengths,|table,offset,out|
        recovered.storage.read(table,offset as usize,out)
            .map_err(|_|novaos_identity::Fault::Unavailable)).map_err(|_|rt::REFUSED)?;
    common::marker(b"NAR2 vue registre isole integre aucune decision locale NovaFS");
    #[cfg(feature="vfs-guest")]
    rt::send_for(rt::config(0),20,&[0],5000)?;
    let port=rt::config(14);if port==0 {return Err(rt::DENIED);}
    let mut sequence=0;
    #[cfg(feature="vfs-guest")]
    let mut pending:Option<crate::recovery_mutation::Pending>=None;
    loop {
        match rt::receive_for(rt::config(1),0) {
            Err(rt::WOULD_BLOCK|rt::TIMED_OUT)=>(),
            #[cfg(feature="vfs-guest")]
            Ok(m) if m.kind==1 && m.tag==21 && m.len==128 && &m.payload[..8]==b"NARCAP01" && m.payload[8..]==[0;120] => {
                // Route du superviseur privé de laboratoire, distincte des
                // mutations utilisateurs. Aucun accès général au volume.
                let result=pending.as_mut().ok_or(rt::DENIED)
                    .and_then(|p|p.backup(recovered.epoch,recovered.recipients));
                let mut a=[0;128];a[..8].copy_from_slice(&result.err().unwrap_or(0).to_le_bytes());
                rt::send_for(rt::config(0),21,&a,5000)?;
            },
            _=>return Ok(())
        }
        let m=match rt::receive_for(port,2) {
            Err(rt::WOULD_BLOCK|rt::TIMED_OUT)=>continue,
            other=>other?
        };
        let mut answer=[0;128];
        let result=(|| {
            if m.kind!=1 || m.len!=128 || m.tag<=sequence {return Err(rt::BAD_ARGUMENT);}
            sequence=m.tag;
            let folder=&m.payload[..8]==b"NA5FOLD1";
            #[cfg(feature="vfs-guest")]
            let mutation=matches!(&m.payload[..8],b"NA5DROP1"|b"NA5ISSU1");
            #[cfg(not(feature="vfs-guest"))]
            let mutation=false;
            if (folder && m.payload[32..]!=[0;96]) ||
               (!folder && !mutation && (&m.payload[..8]!=b"NA3CHECK" || m.payload[24..]!=[0;104])) ||
               (mutation && m.payload[48..]!=[0;80]) {
                return Err(rt::DENIED);
            }
            if folder && !cfg!(feature="vfs-guest") {return Err(rt::DENIED);}
            #[cfg(feature="vfs-guest")]
            if &m.payload[..8]==b"NA5ISSU1" {
                let p=pending.as_mut().ok_or(rt::DENIED)?;
                if m.payload[8..24]!=p.reference() || m.payload[24..32]!=[0;8] || m.payload[32..48]!=p.plan.head().request {return Err(rt::DENIED);}
                // Le vérificateur conserve l'identité du titulaire ayant demandé
                // la mutation ; seul ce canal peut consulter son issue privée.
                p.reconcile()?;
                state=p.activate(&mut recovered.storage)?;
                common::marker(b"NARV retrait UNCERTAIN reconcilie aucune repetition");
                return Ok(());
            }
            // Chaque consultation opérationnelle exige une réponse fraîche.
            // Le reçu historique ne permet jamais d'utiliser cette vue seul.
            let current=crate::tete::query(rt::config(8),&crate::incarnation::identity())?;
            if current[72..80]!=state.head().generation.to_be_bytes() ||
                current[80..112]!=state.head().digest {return Err(rt::DENIED);}
            let grant=state.resolve_verified_factor(m.payload[8..24].try_into().unwrap())
                .map_err(|_|rt::DENIED)?;
            #[cfg(feature="vfs-guest")]
            if &m.payload[..8]==b"NA5DROP1" {
                if pending.is_some() {return Err(rt::DENIED);}
                let object=u64::from_le_bytes(m.payload[24..32].try_into().unwrap());
                let p=crate::recovery_mutation::Pending::new(&state,grant,object,
                    m.payload[32..48].try_into().unwrap(),current,m.payload[8..24].try_into().unwrap())?;
                pending=Some(p);
                let p=pending.as_mut().unwrap();
                p.submit(&mut recovered.storage)?;
                state=p.activate(&mut recovered.storage)?;
                common::marker(b"NARV retrait publie recu et tete authentifies generation suivante");
                return Ok(());
            }
            if folder {
                let object=u64::from_le_bytes(m.payload[24..32].try_into().unwrap());
                let seal=state.folder(grant,object).map_err(|_|rt::DENIED)?;
                answer[8..40].copy_from_slice(&seal);
                answer[40..72].copy_from_slice(&state.head().digest);
                return Ok(());
            }
            answer[8..24].copy_from_slice(&grant.subject());
            answer[24..56].copy_from_slice(&state.head().digest);
            answer[56]=state.groups(grant).map_err(|_|rt::DENIED)?.try_into().map_err(|_|rt::LIMIT)?;
            Ok(())
        })();
        if let Err(code)=result {answer=[0;128];answer[..8].copy_from_slice(&code.to_le_bytes());}
        match rt::send_for(port,m.tag,&answer,25) {
            Ok(())=>(),
            Err(rt::TIMED_OUT)=>(), // consultation ; aucune mutation répétée
            other=>other?
        }
    }
}


// Sans récupération approuvée, aucune vue locale n'est activée. Le vérificateur
// reçoit un refus explicite sur sa route privée et le VFS reste l'effecteur.
#[cfg(feature="vfs-guest")]
pub fn quarantine(_cause:u64)->rt::Result<()> {
    common::marker(b"NARV registre quarantaine generation non activable");
    rt::send_for(rt::config(0),20,&[5],5000)?;
    loop {
        match rt::receive_for(rt::config(1),0) {
            Err(rt::WOULD_BLOCK|rt::TIMED_OUT)=>(),_=>return Ok(())
        }
        let m=match rt::receive_for(rt::config(14),2) {
            Err(rt::WOULD_BLOCK|rt::TIMED_OUT)=>continue,other=>other?
        };
        let mut q=[0;128];q[..8].copy_from_slice(&rt::DENIED.to_le_bytes());
        match rt::send_for(rt::config(14),m.tag,&q,25) {
            Ok(())|Err(rt::TIMED_OUT)=>(),other=>other?
        }
    }
}
