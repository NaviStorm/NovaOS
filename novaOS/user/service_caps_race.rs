//! Scénario du profil 88 sur une copie historique à quatre services en génération 12.
use super::*;

fn message(op: u64, generation: u64) -> svc::Frame {
    let mut f = svc::request(op, 9, 0);
    f[5] = generation;
    f[6] = svc::service_caps::PRIVATE_TMP;
    f
}

fn queue(port: &mut Control, frame: svc::Frame) -> rt::Result<u64> {
    port.tag = port.tag.checked_add(1).ok_or(rt::LIMIT)?;
    rt::send_frame(port.tx, port.tag, &svc::encode(&frame), 0, false)?;
    Ok(port.tag)
}

fn receive(port: &Control, tag: u64, status: u64) -> rt::Result<svc::Frame> {
    let reply = rt::receive_for(port.rx, 15_000)?;
    require(reply.kind == 1 && reply.tag == tag && reply.len == 128)?;
    let frame = svc::decode(&reply.payload).ok_or(rt::BAD_ARGUMENT)?;
    require(frame[0] == svc::VERSION && frame[1] == status && frame[2] == 9)?;
    Ok(frame)
}

pub(super) fn run(fs: &mut fileapi::Client, mut admin: Control, mut read: Control) -> rt::Result<()> {
    let mut ops = Control { tx: rt::config(14), rx: rt::config(15), tag: 0 };
    for port in [&mut read, &mut ops] {
        let tag = queue(port, message(svc::RESTRICT_CAP, 12))?;
        receive(port, tag, rt::DENIED)?;
    }
    let initial = read.call(svc::QUERY_CAPS, 9, 0)?;
    require(initial[5..10] == [1, 1, 12, 12, 1] && initial[11] == rt::OK)?;
    let attempt = read.call(svc::STATUS, 9, 0)?[4];
    // Trois requêtes en vol avant la collecte des réponses, sur deux ports.
    let restrict = queue(&mut admin, message(svc::RESTRICT_CAP, 12))?;
    let restore = queue(&mut admin, message(svc::RESTORE_CAP, 12))?;
    let restart = queue(&mut ops, svc::request(svc::RESTART, 9, 0))?;
    require(receive(&admin, restrict, rt::OK)?[4] == 13)?;
    receive(&admin, restore, protocol::BUSY)?;
    receive(&ops, restart, rt::OK)?;
    let state = read.until(9, |r| r[3] == svc::FAILED || (r[4] > attempt && r[3] == svc::RUNNING))?;
    if state[3] == svc::RUNNING {
        // START a précédé la publication. L'instance en cours garde toute la génération 12.
        let live = read.call(svc::QUERY_CAPS, 9, 0)?;
        require(live[5..11] == [0, 1, 13, 12, 1, 1])?;
        ops.call(svc::RESTART, 9, 0)?;
    }
    read.state(9, svc::FAILED)?;
    require(read.call(svc::QUERY_CAPS, 9, 0)?[5..10] == [0, 0, 13, 0, 0])?;
    require(read.state(10, svc::WAITING)?[5] == 0)?;
    require(read.call(svc::SETTINGS, 10, 0)?[5] == 9)?;
    log(b"SVC-CAP CAS 12 reponses OK BUSY restart concurrence, lecture et exploitation refusees");

    // La révocation intervient après remise, avant l'acquittement sur ce seul port.
    admin.tag = admin.tag.checked_add(1).ok_or(rt::LIMIT)?;
    rt::send_for(admin.tx, admin.tag, &svc::encode(&message(svc::RESTORE_CAP, 13)), 15_000)?;
    require(matches!(rt::receive_for(admin.rx, 15_000), Err(rt::DENIED | rt::CLOSED | rt::REVOKED)))?;
    for _ in 0..2 {
        let relu = read.call(svc::QUERY_CAPS, 9, 0)?;
        require(relu[5..10] == [1, 0, 14, 0, 0] && relu[11] == rt::OK)?;
    }
    require(matches!(admin.submit(svc::SETTINGS, 9, 0), Err(rt::DENIED | rt::CLOSED | rt::REVOKED)))?;
    ops.call(svc::RESTART, 9, 0)?;
    read.state(9, svc::RUNNING)?;
    read.state(10, svc::RUNNING)?;
    require(read.call(svc::QUERY, 9, 0)?[10] == svc::NATIVE_INFO)?;
    log(b"SVC-CAP restauration UNCERTAIN generation 14 relue deux fois, handle revoque NVFS OK");
    ops.call(svc::SHUTDOWN, 0, 0)?;
    rt::value(rt::call(instances::SERVICE_PROCESS, service_process::DRAIN, 0, 0))?;
    fs.close(0)?;
    Ok(())
}
