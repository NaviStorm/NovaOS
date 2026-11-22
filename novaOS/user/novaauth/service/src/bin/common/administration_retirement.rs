//! Acquittements réels des effecteurs et reçu terminal, protocole privé R2.
//! Aucun timeout ni réponse de transport ne vaut retrait durable.
use crate::{common,incarnation,recovery_mutation::Pending,recovery_rpc,runtime as rt,tete};
use novaos_recovery::{administration::{Message,ACKNOWLEDGE,OUTCOME},framing::{MAX,HEADER}};
const UNCERTAIN:u64=22;
fn key()->rt::Result<ed25519_dalek::VerifyingKey> {
    ed25519_dalek::VerifyingKey::from_bytes(include_bytes!(env!("NOVA_DECISION_PUB")))
        .map_err(|_|rt::REFUSED)
}
fn packet<const N:usize>(p:&Pending)->rt::Result<[u8;N]> {
    let h=p.plan.head();
    let current=tete::query(rt::config(8),&incarnation::identity())?;
    if current[72..80]!=h.generation.to_be_bytes() || current[80..]!=h.digest {
        return Err(UNCERTAIN);
    }
    let mut q=[0;N];q[..112].copy_from_slice(&current);
    q[112..128].copy_from_slice(&h.request);Ok(q)
}
pub(super) fn acknowledge(p:&Pending)->rt::Result<()> {
    p.retirement().ok_or(rt::DENIED)?;
    let mut q=packet::<129>(p)?;q[128]=7;
    let m=Message::parse(ACKNOWLEDGE,&q).map_err(|_|rt::BAD_ARGUMENT)?;
    let mut response=[0;MAX];
    let n=recovery_rpc::Client::new(rt::config(8),incarnation::identity())
        .exchange(ACKNOWLEDGE,&q,&mut response).map_err(|_|UNCERTAIN)?;
    if n<8 || common::number(&response[HEADER..])!=0 {return Err(UNCERTAIN);}
    m.acknowledged(&key()?,&response[HEADER+8..HEADER+n]).map_err(|_|UNCERTAIN)?;
    common::marker(b"NAR2 effecteurs acquittes durablement attente issue admissions");Ok(())
}
pub(super) fn terminal(p:&Pending)->rt::Result<()> {
    let mut q=packet::<160>(p)?;q[128..].copy_from_slice(&common::random()?[..]);
    let m=Message::parse(OUTCOME,&q).map_err(|_|rt::BAD_ARGUMENT)?;
    let mut response=[0;MAX];
    let n=recovery_rpc::Client::new(rt::config(8),incarnation::identity())
        .exchange(OUTCOME,&q,&mut response).map_err(|_|UNCERTAIN)?;
    if n<8 || common::number(&response[HEADER..])!=0 {return Err(UNCERTAIN);}
    let (effects,terminal)=m.outcome(&key()?,&response[HEADER+8..HEADER+n]).map_err(|_|UNCERTAIN)?;
    if effects!=7 {return Err(UNCERTAIN);}
    p.verify_retirement(terminal.ok_or(UNCERTAIN)?)?;
    common::marker(b"NAR2 epoque RETIREE recu terminal authentifie");Ok(())
}

/// Consultation pure et fraîche du retrait enregistré. Jamais ACKNOWLEDGE,
/// jamais publication répétée. Le moteur compare ensuite le reçu signé à
/// l'époque, l'empreinte et l'identifiant persistants du sujet désactivé.
pub(super) fn recovered(state:&novaos_identity::recuperation_view::View,
    grant:novaos_identity::Grant,subject:[u8;16],current:[u8;112])
    ->rt::Result<novaos_identity::administration::Retired> {
    let operation=state.retirement_request(grant,subject).map_err(|_|rt::DENIED)?;
    let mut q=[0;160];q[..112].copy_from_slice(&current);
    q[112..128].copy_from_slice(&operation);q[128..].copy_from_slice(&common::random()?[..]);
    let m=Message::parse(OUTCOME,&q).map_err(|_|rt::BAD_ARGUMENT)?;
    let mut response=[0;MAX];
    let n=recovery_rpc::Client::new(rt::config(8),incarnation::identity())
        .exchange(OUTCOME,&q,&mut response).map_err(|_|UNCERTAIN)?;
    if n<8 || common::number(&response[HEADER..])!=0 {return Err(UNCERTAIN);}
    let key=key()?;
    let (effects,terminal)=m.outcome(&key,&response[HEADER+8..HEADER+n]).map_err(|_|rt::DENIED)?;
    if effects!=7 {return Err(UNCERTAIN);}
    let receipt=terminal.ok_or(UNCERTAIN)?;
    novaos_identity::administration::Retired::verify(incarnation::SERVER,&key,&receipt[..128],&receipt[128..])
        .map_err(|_|rt::DENIED)
}
