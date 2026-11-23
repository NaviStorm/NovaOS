//! Réception R1 dans le vérificateur QEMU. La route vient exclusivement du
//! noyau ; le client ne choisit ni titulaire, ni session, ni installation.
use super::*;
#[path = "../../../../../../../components/exec-client/src/admin_wire.rs"]
mod admin_wire;
#[path = "../../../../../../../components/exec-client/src/admin_command.rs"]
mod admin_command;
use admin_wire::{Direction, Inbox};
use novaos_auth_session::epoch_sessions::EpochBinding;
const LIST: u64 = 0x4e5841444c495354;
const META: u64 = 0x4e5841444d455441;
struct Lane {
    key: u64,
    inbox: Inbox,
}
pub(super) struct Channels {
    lanes: [Lane; 2],
}
fn port() -> rt::Result<u64> {
    let p = unsafe {
        (&*(0x401000 as *const core::sync::atomic::AtomicU64).add(21))
            .load(core::sync::atomic::Ordering::Acquire)
    };
    if p == 0 { Err(rt::DENIED) } else { Ok(p) }
}
pub(super) fn binding(tag: u64, reference: [u8; 16]) -> rt::Result<EpochBinding> {
    let mut q = [0; 128];
    q[..8].copy_from_slice(b"NA3CHECK");
    q[8..24].copy_from_slice(&reference);
    let p = port().map_err(|e| {trace(20,e);e})?;
    let a = rpc(p, p, tag, &q).map_err(|e| {trace(21,e);e})?;
    if number(&a) != rt::OK || number(&a[64..]) == 0 {
        trace(if number(&a)!=0 {22}else{23},number(&a));
        return Err(rt::DENIED);
    }
    Ok(EpochBinding {
        binding: novaos_auth_session::Binding {
            domain: DOMAIN,
            reference,
            subject: a[8..24].try_into().unwrap(),
            head: a[24..56].try_into().unwrap(),
        },
        epoch: number(&a[64..]),
    })
}
fn exchange(login: &mut Login, q: &[u8; 128]) -> rt::Result<[u8; 128]> {
    login.store_seq = login.store_seq.checked_add(1).ok_or(rt::LIMIT)?;
    let p = port()?;
    let a = rpc(p, p, login.store_seq, q)?;
    if number(&a) != 0 {
        return Err(number(&a));
    }
    Ok(a)
}
pub(super) fn authorize_installation(
    login: &mut Login,
    nonce: [u8; 16],
    sid: [u8; 32],
    d: &[u8; 240],
) -> rt::Result<()> {
    let current = login.inspected_binding(sid)?;
    if current.head != d[8..40] || current.subject != d[40..56] {
        return Err(rt::REVOKED);
    }
    let mut q = [0; 128];
    q[..8].copy_from_slice(b"NXASCP01");
    q[8..24].copy_from_slice(&current.reference);
    q[24..56].copy_from_slice(&current.head);
    q[56..72].copy_from_slice(&d[56..72]);
    q[72..104].copy_from_slice(&d[152..184]);
    let a = exchange(login, &q)?;
    let scope = number(&a[8..]);
    if scope == 0 {
        return Ok(());
    }
    if scope > 15 || number(&a[16..]) != current.epoch {
        return Err(rt::DENIED);
    }
    let mut q = [0; 128];
    q[..8].copy_from_slice(b"NXLDADM1");
    q[8..24].copy_from_slice(&nonce);
    q[24..56].copy_from_slice(&current.head);
    q[56..88].copy_from_slice(&sid);
    q[88..104].copy_from_slice(&d[56..72]);
    q[104..112].copy_from_slice(&scope.to_le_bytes());
    q[112..120].copy_from_slice(&current.epoch.to_le_bytes());
    rt::send_for(rt::config(5), 1, &q, 250)
}
fn bytes<const N: usize>(words: &[u64]) -> [u8; N] {
    let mut bytes = [0; N];
    for (w, b) in words.iter().zip(bytes.chunks_exact_mut(8)) {
        b.copy_from_slice(&w.to_le_bytes());
    }
    bytes
}
impl Channels {
    pub(super) fn new() -> Self {
        Self {
            lanes: core::array::from_fn(|_| Lane {
                key: 0,
                inbox: Inbox::new(),
            }),
        }
    }
    pub(super) fn poll(&mut self, login: &mut Login,
                     #[cfg(feature="administration-r2-guest")] peer:&mut Login) {
        for i in 0..2 {
            let r = rt::call(rt::IPC_NOTIFY, rt::config(5), i as u64, LIST);
            let key = if r.status == 0 { r.value } else { 0 };
            let lane = &mut self.lanes[i];
            if key != lane.key {
                lane.inbox.close();
                *lane = Lane {
                    key,
                    inbox: Inbox::new(),
                };
            }
            if key == 0 {
                continue;
            }
            let m = match rt::receive_for(key, 0) {
                Err(rt::WOULD_BLOCK | rt::TIMED_OUT) => continue,
                Ok(m) => m,
                Err(_) => {
                    lane.inbox.close();
                    let _ = rt::call(rt::IPC_CLOSE, key, 0, 0);
                    continue;
                }
            };
            let mut answer = [0; 128];
            let ready = if m.kind == 1 && m.len == 128 && number(&m.payload[8..]) == m.tag {
                lane.inbox.accept(&m.payload, Direction::Request)
            } else {
                Err(admin_wire::Error::Format)
            };
            if ready == Ok(false) {
                let _ = rt::send_for(key, m.tag, &[0; 8], 25);
                continue;
            }
            let result = (|| {
                ready.map_err(|_| rt::BAD_ARGUMENT)?;
                let (_, request) = lane.inbox.message().map_err(|_| rt::BAD_ARGUMENT)?;
                if request.len() != 128 {
                    return Err(rt::BAD_ARGUMENT);
                }
                // Le client ne peut pas injecter NXACTX01/NXASCP01 sur la route
                // privée du registre. Le décodage côté client n'est pas fiable.
                admin_command::decode(request.try_into().map_err(|_| rt::BAD_ARGUMENT)?)
                    .map_err(|_| rt::BAD_ARGUMENT)?;
                let mut meta = [0; 31];
                for (i, w) in meta.iter_mut().enumerate() {
                    *w = rt::value(rt::call(rt::IPC_NOTIFY, key, i as u64, META))?;
                }
                let sid = bytes::<32>(&meta[5..9]);
                let current = login.inspected_binding(sid)?;
                if bytes::<32>(&meta[23..27]) != login.route.issuer
                    || bytes::<16>(&meta[9..11]) != current.subject
                    || bytes::<16>(&meta[13..15]) != current.domain
                    || meta[28] != current.epoch
                    || meta[27] == 0
                    || meta[27] > 15
                    || now()? >= meta[29]
                {
                    return Err(rt::REVOKED);
                }
                let mut context = [0; 128];
                context[..8].copy_from_slice(b"NXACTX01");
                context[8..24].copy_from_slice(&current.reference);
                context[24..56].copy_from_slice(&current.head);
                context[56..72].copy_from_slice(&bytes::<16>(&meta[11..13]));
                context[72..104].copy_from_slice(&bytes::<32>(&meta[15..19]));
                context[104..112].copy_from_slice(&meta[27].to_le_bytes());
                context[112..120].copy_from_slice(&current.epoch.to_le_bytes());
                exchange(login, &context)?;
                #[cfg(feature="administration-r2-guest")]
                if !matches!(admin_command::decode(request),Ok(admin_command::Command::Read(_))) {
                    // Aucun de ces champs ne vient du paquet ELF : sid a été
                    // inspecté et la route appartient à cette instance nsh.
                    let mut binding=[0;128];binding[..8].copy_from_slice(b"NXABND02");
                    binding[8..40].copy_from_slice(&sid);
                    binding[40..72].copy_from_slice(&login.route.client);
                    exchange(login,&binding)?;
                }
                #[cfg(feature="administration-r2-guest")]
                if matches!(admin_command::decode(request),Ok(admin_command::Command::Confirm{..})) {
                    let mut pre:[u8;128]=request.try_into().unwrap();
                    pre[..8].copy_from_slice(b"NXRTPRE2");
                    let selected=exchange(login,&pre)?;
                    if selected[8..24]!=[0;16] {
                        #[cfg(feature="membership-r2-guest")]
                        let selective=&selected[48..56]==b"NXMGSEL1";
                        #[cfg(not(feature="membership-r2-guest"))]
                        let selective=false;
                        if selective {
                            #[cfg(feature="membership-r2-guest")]
                            inspect_membership(login,peer,&selected,request)?;
                        }else{
                        if selected[48..]!=[0;80] || selected[32..48]!=request[16..32] {
                            return Err(rt::DENIED);
                        }
                        let subject=selected[8..24].try_into().unwrap();
                        let epoch=number(&selected[24..]);
                        retire_subject(login,peer,subject,epoch)?;
                        }
                        pre[..8].copy_from_slice(b"NXRTACK2");
                        if exchange(login,&pre)?!=selected {return Err(rt::DENIED);}
                    }
                }
                answer = exchange(login, request.try_into().unwrap())?;
                Ok(())
            })();
            if let Err(code) = result {
                answer.fill(0);
                answer[..8].copy_from_slice(&code.to_le_bytes());
            }
            lane.inbox.discard();
            for offset in (0..answer.len()).step_by(admin_wire::PAYLOAD) {
                let packet = match admin_wire::frame(m.tag, Direction::Response, &answer, offset) {
                    Ok(p) => p,
                    Err(_) => break,
                };
                if rt::send_for(key, m.tag, &packet, 25).is_err() {
                    break;
                }
            }
        }
    }
}

#[cfg(feature="administration-r2-guest")]
fn retire_subject(login:&mut Login,peer:&mut Login,subject:[u8;16],epoch:u64)->rt::Result<()> {
    if subject==[0;16] || epoch==0 {return Err(rt::DENIED);}
    // Le registre désigne le sujet ; le client ne choisit aucune route. La table
    // commune empêche aussi l'ouverture d'une preuve vérifiée avant la clôture.
    login.sessions.borrow_mut().retire(DOMAIN,subject,epoch).map_err(|_|rt::REFUSED)?;
    let mut store_seq=login.store_seq;
    let mut relay_seq=login.relay_seq;
    fn retire_route(actor:&mut Login,subject:[u8;16],epoch:u64,
                    store_seq:&mut u64,relay_seq:&mut u64)->rt::Result<()> {
        if let Some((holder,old_epoch,sid))=actor.retirement_binding {
            if holder==subject && old_epoch<=epoch {
                actor.store_seq=*store_seq;actor.relay_seq=*relay_seq;
                let closed=actor.close(sid);
                *store_seq=actor.store_seq;*relay_seq=actor.relay_seq;
                closed?;
                // CLOSED/REVOKED d'un transport n'est pas un acquittement de
                // l'effecteur. Seul NXRSTOP1 effectivement accepté fait foi.
                if actor.kernel_retired!=Some(sid) || actor.last_closed!=Some(sid) {
                    return Err(22);
                }
            }
        }
        Ok(())
    }
    let result=retire_route(login,subject,epoch,&mut store_seq,&mut relay_seq)
        .and_then(|()|retire_route(peer,subject,epoch,&mut store_seq,&mut relay_seq));
    login.store_seq=store_seq;login.relay_seq=relay_seq;
    peer.store_seq=store_seq;peer.relay_seq=relay_seq;
    result?;
    marker(b"NAR2 retrait local sujet verificateur lanceur VFS acquittes");Ok(())
}

/// Diagnostic borné, une ligne sur la seule tentative explicite de reconnexion.
/// Étapes 1=fermeture relais, 2=transport abandon paquet, 3=réponse abandon,
/// 10=clé publique du facteur, 11=lecture de la liaison actuelle. Aucun secret.
pub(super) fn trace(stage:u64, code:u64) {
    let mut out=*b"NXADMIN ouverture etape=0000000000000000 statut=0000000000000000";
    let hex=b"0123456789abcdef";
    for (start,value) in [(24,stage),(48,code)] {
        for i in 0..16 {out[start+i]=hex[((value>>((15-i)*4))&15) as usize];}
    }
    let _=rt::call(rt::DEBUG_WRITE,out.as_ptr() as u64,out.len() as u64,0);
}

#[cfg(feature="membership-r2-guest")]
fn inspect_membership(login:&mut Login,peer:&mut Login,q:&[u8;128],request:&[u8])->rt::Result<()> {
    if q[56..72]!=request[16..32] || q[72..104]!=login.route.issuer || q[104..]!=[0;24]{return Err(rt::DENIED);}
    let subject=q[8..24].try_into().unwrap();let group=q[24..40].try_into().unwrap();
    let revision=number(&q[40..]);let operation=q[56..72].try_into().unwrap();
    let mut seen=false;let mut sequence=login.relay_seq.max(peer.relay_seq);
    let mut inspect=|actor:&mut Login|->rt::Result<()> {
        if actor.retirement_binding.is_some_and(|(u,_,_)|u==subject){
            actor.relay_seq=sequence;
            let result=membership_exec::independent(actor,subject,group,revision,operation);
            sequence=actor.relay_seq;seen=true;result?;
        }Ok(())
    };
    let result=inspect(login).and_then(|_|inspect(peer));
    login.relay_seq=sequence;peer.relay_seq=sequence;
    result?;if seen{Ok(())}else{Err(22)}
}
