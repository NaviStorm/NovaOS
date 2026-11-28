#![no_std]

use core::{panic::PanicInfo, slice};
use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

const PUBLIC: &[u8; 32] = include_bytes!("../diagnostic-5m/autorite-laboratoire.pub");
const HEADER: usize = 16 + 16 + 4 * 32 + 16 * 8;
const MAGIC: &[u8; 16] = b"NOVA5Q-USB-V1!!!";

/// Appel interne du BDS. Les pointeurs proviennent de sa propre allocation et
/// des lectures bornées de l'ESP, avant de donner la main au PE.
#[no_mangle]
pub unsafe extern "win64" fn nova_verifier_usb(
    envelope: *const u8,
    n: usize,
    epoch: *const u8,
    facts: *const u64,
    esp: *const u8,
    esp_len: usize,
    off: usize,
    size: usize,
    path: *const u8,
    path_len: usize,
) -> usize {
    if !(HEADER + 65..=12000).contains(&n)
        || !(1 << 20..=64 << 20).contains(&esp_len)
        || !(512..=8 << 20).contains(&size)
        || size > esp_len
        || off > esp_len - size
        || !(4..=128).contains(&path_len)
    {
        return 0;
    }
    let bytes = slice::from_raw_parts(envelope, n);
    let (body, sig) = bytes.split_at(n - 64);
    let Ok(key) = VerifyingKey::from_bytes(PUBLIC) else {
        return 0;
    };
    let Ok(signature) = Signature::from_slice(sig) else {
        return 0;
    };
    if key.verify_strict(body, &signature).is_err() {
        return 0;
    }
    let media = slice::from_raw_parts(esp, esp_len);
    let record = &body[HEADER..];
    if &body[..16] != MAGIC
        || body[16..32] != *slice::from_raw_parts(epoch, 16)
        || body[32..64] != Sha256::digest(media)[..]
        || body[64..96] != Sha256::digest(&media[off..off + size])[..]
        || body[96..128] != Sha256::digest(slice::from_raw_parts(path, path_len))[..]
        || body[128..160] != Sha256::digest(record)[..]
    {
        return 0;
    }
    for (i, actual) in slice::from_raw_parts(facts, 16).iter().enumerate() {
        if body[160 + i * 8..168 + i * 8] != actual.to_le_bytes() {
            return 0;
        }
    }
    1
}

#[panic_handler]
fn stop(_: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
