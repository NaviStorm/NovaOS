use crate::runtime as rt;
pub const TTL_MS: u32 = 1500;
pub const DOMAIN: [u8; 16] = [7; 16];
pub fn marker(bytes: &[u8]) {
    #[cfg(feature="vfs-guest")]
    {
        // Une seule occurrence des succès répétitifs du profil. Les erreurs ne
        // sont jamais filtrées. Préserver le quota DEBUG=16 et réserver les
        // mesures de fin de capture au lieu d'épuiser le quota en consultations.
        use core::sync::atomic::{AtomicU64,Ordering};
        static SEEN:AtomicU64=AtomicU64::new(0);
        let bit=match bytes {
            b"NAUTH signataire NOW autorise gestion et arguments refuses 8/8"=>1,
            b"NAUTH TLS avant"=>2,b"NAUTH TLS actif"=>4,b"NAUTH TLS apres"=>8,
            b"NAUTH transport ferme"=>16,b"NAUTH tete signature et portee verifiees"=>32,
            b if b.starts_with(b"NAUTH TLS debut=")=>64,
            _=>0,
        };
        if bit!=0 && SEEN.fetch_or(bit,Ordering::Relaxed)&bit!=0 {return;}
    }
    rt::call(
        rt::DEBUG_WRITE,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
    );
}
pub fn remote(rx: u64, tag: u64) -> rt::Result<rt::ReceiveV1> {
    let msg = rt::receive_for(rx, 5000)?;
    if msg.kind != 1 || msg.tag != tag || msg.len != 128 {
        return Err(rt::BAD_ARGUMENT);
    }
    Ok(msg)
}
pub fn number(data: &[u8]) -> u64 {
    u64::from_le_bytes(data[..8].try_into().unwrap())
}
pub fn rpc(tx: u64, rx: u64, tag: u64, frame: &[u8; 128]) -> rt::Result<[u8; 128]> {
    rt::send_for(tx, tag, frame, 5000)?;
    Ok(remote(rx, tag)?.payload)
}
// Les deux ELF sont exécutés avec QEMU -cpu max. Une lecture RNDR absente
// lève une faute utilisateur sur une autre CPU. Aucun repli prédictible.
pub fn random() -> rt::Result<zeroize::Zeroizing<[u8; 32]>> {
    let mut bytes = zeroize::Zeroizing::new([0u8; 32]);
    for part in bytes.chunks_exact_mut(8) {
        let n = (0..10).find_map(|_| word()).ok_or(rt::REFUSED)?;
        part.copy_from_slice(&n.to_le_bytes());
    }
    if bytes.chunks_exact(8).all(|s| s == &bytes[..8]) {
        return Err(rt::REFUSED);
    }
    Ok(bytes)
}
#[cfg(target_arch = "x86_64")]
fn word() -> Option<u64> {
    use core::arch::x86_64::{__cpuid, _rdrand64_step};
    if __cpuid(1).ecx & (1 << 30) == 0 {
        return None;
    }
    let mut n = 0;
    (unsafe { _rdrand64_step(&mut n) } == 1).then_some(n)
}
#[cfg(target_arch = "aarch64")]
fn word() -> Option<u64> {
    let n: u64;
    let valid: u64;
    unsafe {
        core::arch::asm!("mrs {v}, S3_3_C2_C4_0","cset {ok}, ne",v=out(reg)n,ok=out(reg)valid,options(nomem,nostack))
    };
    (valid != 0).then_some(n)
}
