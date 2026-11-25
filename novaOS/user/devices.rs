#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "../abi/native.rs"]
mod abi;
#[allow(dead_code)]
#[path = "../abi/block.rs"]
mod block_wire;
mod device_inventory;
#[allow(dead_code)]
mod device_lifecycle;
mod gate;
#[cfg(virtio_net_probe)]
#[allow(dead_code)]
mod virtio_net;
#[cfg(materiel_lab)]
mod edu_manager;
use device_inventory as wire;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".section .text.demo,\"ax\"",
    ".global _start",
    "_start:",
    "call run",
    "ud2"
);
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".section .text.demo,\"ax\"",
    ".global _start",
    "_start:",
    "bl run",
    "brk #0"
);

fn diagnostic(bytes: &[u8]) -> bool {
    gate::call(
        abi::DEBUG_WRITE,
        bytes.as_ptr() as u64,
        bytes.len() as u64,
        0,
    )
    .status
        == abi::OK
}

fn inspect(page: &[u8]) -> bool {
    if page[..4] != *wire::MAGIC || page[4] as usize > wire::MAX_DEVICES {
        return false;
    }
    let records = wire::HEADER + 4 * usize::from(page[5]);
    let Some(records) = page.get(records..records + usize::from(page[4]) * wire::RECORD) else {
        return false;
    };
    // Le catalogue physique de phase 1 est vide. Un identifiant reconnu
    // ne suffit jamais à fournir une autorisation d'exécution.
    let catalog: &[device_lifecycle::Driver] = &[];
    let mut seen = 0u8;
    for r in records.chunks_exact(wire::RECORD) {
        let vendor = u16::from_le_bytes([r[4], r[5]]);
        let device = u16::from_le_bytes([r[6], r[7]]);
        let location = u32::from_le_bytes([r[3], r[2], r[0], r[1]]);
        let observed = device_lifecycle::Device::observed(location, vendor, device);
        if observed.matching(catalog).is_some() {
            return false;
        }
        if wire::virtio_net(vendor, device, r[8], r[9]) {
            if !diagnostic(b"VirtIO-net PCI moderne inventorie") {
                return false;
            }
            #[cfg(virtio_net_probe)]
            if !virtio_net::probe::run() || !diagnostic(b"VirtIO-net files logicielles ok RX=100000 TX=100000 DMA=0") {
                return false;
            }
        }
        if let Some((i, name)) = wire::model(vendor, device) {
            if seen & (1 << i) == 0 && !diagnostic(name) {
                return false;
            }
            seen |= 1 << i;
        }
    }
    for operation in [
        abi::IPC_BOOTSTRAP,
        block_wire::PORTAL,
        block_wire::PAGE_PORTAL,
    ] {
        if gate::call(operation, 0, 0, 0).status != abi::DENIED {
            return false;
        }
    }
    let mut summary = *b"MATERIEL gestionnaire inventaire lu; pilotes natifs actifs=0 droits-materiel=aucun fonctions=0x00";
    let hex = b"0123456789abcdef";
    let end = summary.len();
    summary[end - 2] = hex[usize::from(page[4] >> 4)];
    summary[end - 1] = hex[usize::from(page[4] & 15)];
    diagnostic(&summary)
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    // Sûreté : le lanceur fournit une page PRIVÉE entière, initialisée, RO/NX.
    let page = unsafe { core::slice::from_raw_parts(0x600000 as *const u8, wire::PRIVATE_PAGE) };
    #[cfg(materiel_lab)]
    if edu_manager::offer(page) {
        exit(if edu_manager::supervise(page) { 0 } else { 1 });
    }
    let code = if inspect(page) { 0 } else { 1 };
    exit(code)
}
fn exit(code: u64) -> ! {
    gate::call(abi::PROCESS_EXIT, code, 0, 0);
    loop {
        core::hint::spin_loop()
    }
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    exit(255)
}
