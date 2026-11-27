//! Contrat DATA du lanceur mono-groupe. Les effets matériels appartiennent au superviseur 3E.
use crate::runtime::*;
use novaos_gestionnaire_sim::ipc::{Body, Op, Reply as Response, Request, Status};

fn requete(op: Op, id: u64, generation: u64) -> Request {
    let epoch = config(2) | (1 << 63);
    let recherche = matches!(op, Op::Receipt | Op::Ack);
    Request {
        op,
        epoch,
        id,
        revision: config(2),
        body: Body {
            generation,
            a: if recherche { epoch } else { 0 },
            b: if recherche { 2 } else { 0 },
            c: 0,
        },
    }
}
pub(crate) fn appel(ports: (u64, u64), r: Request) -> Result<Response> {
    let mut buf = [0; 128];
    let n = r.encode(&mut buf).map_err(|_| REFUSED)?;
    send_for(ports.0, r.epoch, &buf[..n], 2500)?;
    let msg = receive_for(ports.1, 2500)?;
    if msg.kind != 1 || msg.tag != r.epoch || msg.len != 88 {
        return Err(REFUSED);
    }
    let reply = Response::decode(r, &msg.payload[..88]).map_err(|_| REFUSED)?;
    if reply.status != Status::Confirmed {
        return Err(REFUSED);
    }
    Ok(reply)
}
fn attendre(mot: usize, valeur: u64) {
    while config(mot) != valeur {
        call(THREAD_YIELD, 0, 0, 0);
    }
}
fn generation() -> u64 {
    loop {
        let g = config(12);
        if g != 0 {
            return g;
        }
        call(THREAD_YIELD, 0, 0, 0);
    }
}
#[allow(dead_code)]
pub(crate) fn conduire(plan: [u8; 32]) -> Result<()> {
    let g = generation();
    let ports = (config(8), config(9));
    let prepare = appel(ports, requete(Op::Prepare, 1, g))?;
    if prepare.digest != plan || prepare.generation != g {
        return Err(REFUSED);
    }
    let launch = requete(Op::Launch, 2, g);
    let first = appel(ports, launch)?;
    // Perte logique de la réponse : consultation du reçu puis réémission de la demande initiale.
    let receipt = appel(ports, requete(Op::Receipt, 3, g))?;
    if receipt != first
        || appel(ports, launch)? != receipt
        || receipt.value == 0
        || receipt.generation != g
        || u64::from_le_bytes(receipt.digest[..8].try_into().unwrap()) == 0
    {
        return Err(REFUSED);
    }
    appel(ports, requete(Op::Ack, 4, g))?;
    Ok(())
}
fn recevoir(attendu: Request) -> Result<Request> {
    let m = receive_for(config(8), 2500)?;
    if m.kind != 1 || m.tag != attendu.epoch || m.len > 128 {
        return Err(REFUSED);
    }
    let r = Request::decode(&m.payload[..m.len as usize]).map_err(|_| REFUSED)?;
    if r != attendu {
        return Err(REFUSED);
    }
    Ok(r)
}
fn repondre(r: Request, rep: Response) -> Result<()> {
    send_for(config(9), r.epoch, &rep.encode(r), 2500)
}
fn signal(tag: u64, r: Request) -> Result<()> {
    let mut d = [0u8; 16];
    d[..8].copy_from_slice(&r.epoch.to_le_bytes());
    d[8..].copy_from_slice(&r.id.to_le_bytes());
    send_frame(config(10), tag, &d, 0, false)
}
#[allow(dead_code)]
pub(crate) fn servir(plan: [u8; 32]) -> Result<()> {
    let g = generation();
    let p = recevoir(requete(Op::Prepare, 1, g))?;
    signal(5, p)?;
    attendre(13, 1); // Le superviseur a scellé les octets du paquet.
    let mut prepared = Response::confirmed(p.revision, g);
    prepared.digest = plan;
    repondre(p, prepared)?;
    let launch = recevoir(requete(Op::Launch, 2, g))?;
    signal(6, launch)?;
    attendre(13, 2); // Publication effective du processus et de ses capacités.
    let mut recu = Response::confirmed(config(14), g);
    recu.digest[..8].copy_from_slice(&config(15).to_le_bytes());
    repondre(launch, recu)?;
    // Le slot de reçu est réservé jusqu'à l'ACK. Un rejeu ne produit aucun second dépôt.
    let req = recevoir(requete(Op::Receipt, 3, g))?;
    repondre(req, recu)?;
    repondre(recevoir(launch)?, recu)?;
    let ack = recevoir(requete(Op::Ack, 4, g))?;
    repondre(ack, Response::confirmed(ack.revision, g))?;
    signal(7, ack)?;
    attendre(16, 1);
    Ok(())
}
