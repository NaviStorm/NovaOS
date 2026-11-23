#![no_std]
#![no_main]
mod common;
#[cfg(feature="exec-admission-guest")]
#[path="common/exec_admission_guest.rs"]
mod exec_admission_guest;
#[cfg(feature="vfs-guest")]
#[path="common/recovery_capture.rs"]
mod recovery_capture;
#[cfg(feature="vfs-guest")]
#[path="common/recovery_mutation.rs"]
mod recovery_mutation;
#[cfg(feature="membership-r2-guest")]
#[path="common/membership_transaction.rs"]
mod membership_transaction;
#[cfg(all(feature="recuperation-guest",not(feature="vue-guest")))]
#[path="common/no_heap.rs"]
mod no_heap;
#[cfg(feature="vue-guest")]
#[path="common/view_heap.rs"]
mod view_heap;
#[cfg(feature="vue-guest")]
#[path="common/recovery_view.rs"]
mod recovery_view;
#[cfg(feature="recuperation-guest")]
#[path="common/recovery_rpc.rs"]
mod recovery_rpc;
#[cfg(feature="recuperation-guest")]
#[path="common/recovery_guest.rs"]
mod recovery_guest;
#[cfg(feature="triplet-guest")]
#[path="common/recovery_store.rs"]
mod recovery_store;
#[path = "common/incarnation.rs"]
mod incarnation;
#[cfg(feature="tete-guest")]
#[path="common/tete.rs"]
mod tete;
#[allow(dead_code)]
#[path = "../../../../storage/runtime.rs"]
mod runtime;
use ed25519_dalek::{Signature, VerifyingKey};
use runtime as rt;

fn qualify() -> rt::Result<()> {
    let public = VerifyingKey::from_bytes(include_bytes!(env!("NOVA_QEMU_SERVER_PUB")))
        .map_err(|_| rt::REFUSED)?;
    let id = incarnation::identity();
    if id == [0; 32] {
        return Err(rt::REFUSED);
    }
    // Le registre démarre avant le signataire. Le dernier mot publié est la feuille IPC.
    let port = loop {
        let key = rt::config(8);
        if key != 0 {
            break key;
        }
        match rt::receive_for(rt::config(1), 10) {
            Err(rt::TIMED_OUT) => (),
            _ => return Err(rt::CLOSED),
        }
    };
    let nonce = common::random()?;
    let mut req = [0; 128];
    req[..16].copy_from_slice(&incarnation::SERVER);
    req[16..32].copy_from_slice(&incarnation::DOMAIN);
    req[32..64].copy_from_slice(&id);
    req[64..96].copy_from_slice(&nonce[..]);
    for field in [0, 16, 32] {
        req[field] ^= 1;
        let response = common::rpc(port, port, incarnation::TAG, &req)?;
        if common::number(&response) != rt::DENIED || response[8..] != [0; 120] {
            return Err(rt::REFUSED);
        }
        req[field] ^= 1;
    }
    let response = common::rpc(port, port, incarnation::TAG, &req)?;
    if common::number(&response) != rt::OK {
        return Err(rt::DENIED);
    }
    let signature = Signature::from_slice(&response[8..72]).map_err(|_| rt::REFUSED)?;
    public
        .verify_strict(&incarnation::transcript(&id, &nonce), &signature)
        .map_err(|_| rt::REFUSED)?;
    let replay = common::rpc(port, port, incarnation::TAG, &req)?;
    if common::number(&replay) != rt::DENIED || replay[8..] != [0; 120] {
        return Err(rt::REFUSED);
    }
    common::marker(b"NAUTH incarnation preuve invitee OK substitutions=3 rejeu=refuse");
    #[cfg(all(feature="recuperation-guest",not(feature="vue-guest")))]
    recovery_guest::receive(port,&id)?;
    #[cfg(feature="vue-guest")]
    {
        let recovered = match recovery_guest::receive(port,&id) {
            Ok(value)=>value,
            #[cfg(feature="vfs-guest")]
            Err(code)=>return recovery_view::quarantine(code),
            #[cfg(not(feature="vfs-guest"))]
            Err(code)=>return Err(code),
        };
        // Les clés de remise sont déjà détruites. Aucun HEAP_RESIZE n’est accordé.
        view_heap::enable();
        return recovery_view::serve(recovered);
    }
    #[cfg(all(feature="tete-guest",not(feature="recuperation-guest")))]
    match tete::query(port, &id) {
        Ok(_) => common::marker(b"NAUTH registre tete courante"),
        Err(rt::DENIED) => common::marker(b"NAUTH registre tete route retiree"),
        Err(_) => common::marker(b"NAUTH registre tete non verifiable"),
    }
    #[cfg(all(feature = "horloge-guest", not(feature="tete-guest")))]
    {
        let mut frame = [0; 128];
        frame[..32].copy_from_slice(&id);
        rt::send_for(port, incarnation::EXCHANGE, &frame, 2000)?;
        let response = rt::receive_for(port, 12000)?;
        if response.kind != 1 || response.len != 128 || response.tag != incarnation::EXCHANGE {
            return Err(rt::REFUSED);
        }
        match common::number(&response.payload) {
            rt::OK => common::marker(b"NAUTH registre canal confirme"),
            rt::REFUSED => common::marker(b"NAUTH registre canal refuse"),
            _ => return Err(rt::REFUSED),
        }
    }
    loop {
        match rt::receive_for(rt::config(1), 500) {
            Err(rt::TIMED_OUT) => (),
            _ => return Ok(()),
        }
    }
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let status=qualify().err().unwrap_or(0);
    #[cfg(feature="recuperation-guest")]
    {
        let mut message=*b"NAR2 registre termine statut=0000000000000000";
        let start=message.len()-16;
        for i in 0..16 {
            message[start+i]=b"0123456789abcdef"[((status>>(60-4*i))&15) as usize];
        }
        common::marker(&message);
    }
    rt::exit(status)
}
