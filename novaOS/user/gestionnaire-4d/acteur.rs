#![no_std]
#![no_main]
extern crate alloc;
mod lancement;
mod mesures_tas;

#[path = "../heap.rs"]
mod heap;
#[allow(dead_code)]
#[path = "../storage/runtime.rs"]
mod runtime;
fn allocation_probe() {}
use alloc::vec;
use core::sync::atomic::{AtomicU64, Ordering};
use novaos_gestionnaire_sim::{
    catalogue::{Blob, Bus, Catalog, Limits, Origin, Policy, Signer, Target},
    inventory::{Address, Node, Ownership, Slot},
    ipc::{Binding, Op, Request, Role, Status},
    manager::{Manager, Selection},
    resources::Use,
};
use runtime::*;
use sha2::{Digest, Sha256};

macro_rules! object {
    ($n:literal) => {
        include_bytes!(concat!(env!("NOVA_4D_OBJETS"), "/", $n))
    };
}
macro_rules! anchor {
    ($n:literal) => {
        *include_bytes!(concat!("../../libs/catalogue-pilotes/vecteurs/", $n))
    };
}
fn trust() -> novaos_gestionnaire_sim::catalogue::Result<Policy> {
    let signer = Signer {
        publisher: "org.novaos.lab".into(),
        key_id: "lab-v1".into(),
        origin: Origin::Laboratoire,
        manifest_key: anchor!("pilote.pub"),
        elf_key: anchor!("elf.pub"),
        first: 1,
        last: 4,
        revoked: config(5) == 7,
        priority: 1,
    };
    let mut p = Policy::new(
        1,
        if config(5) == 8 {
            Origin::Distribution
        } else {
            Origin::Laboratoire
        },
        3,
        vec![signer],
        anchor!("systeme.pub"),
    )?;
    p.add_resource(
        object!("profil.toml"),
        object!("profil.bin"),
        object!("profil.sig"),
    )?;
    Ok(p)
}
fn mark(index: usize, value: u64) {
    // La zone de bilan appartient au gestionnaire. Publication avant le rendez-vous.
    unsafe { (&*(0x401000 as *const AtomicU64).add(index)).store(value, Ordering::Release) }
}
fn checkpoint(tag: u64, epoch: u64) -> Result<()> {
    let mut body = [0u8; 16];
    body[..8].copy_from_slice(&epoch.to_le_bytes());
    body[8..].copy_from_slice(&1u64.to_le_bytes());
    send_frame(config(6), tag, &body, 0, false)
}
fn gate() -> Result<[u8; 32]> {
    let epoch = config(2);
    let id = config(3);
    let sid = config(4);
    if epoch == 0 || id != 0x10411af4 || sid != 56 {
        return Err(REFUSED);
    }
    checkpoint(1, epoch)?;
    let policy = trust().map_err(|_| REFUSED)?;
    let policy_bytes = mesures_tas::live() + core::mem::size_of_val(&policy);
    let mut catalogue = Catalog::new(&policy);
    let mut signature = *object!("pilote.sig");
    if config(5) == 1 {
        signature[0] ^= 1
    }
    catalogue
        .admit(
            object!("pilote.toml"),
            &signature,
            &[Blob {
                isa: "aarch64",
                bytes: object!("pilote.pkg01"),
            }],
        )
        .map_err(|_| REFUSED)?;
    checkpoint(2, epoch)?;
    let slot = Slot::new(7).map_err(|_| REFUSED)?;
    let bus = Bus::Pci {
        vendor: Some(id as u16),
        device: Some((id >> 16) as u16),
        subsystem_vendor: None,
        subsystem_device: None,
        revision: None,
        class_base: Some(2),
        subclass: Some(0),
    };
    let mut manager = Manager::new(config(7), &catalogue, 1, Use::LAB, Use::default(), None, 0)
        .map_err(|_| REFUSED)?;
    manager
        .refresh(
            epoch,
            true,
            [Node {
                slot,
                controller: 1,
                appearance: epoch,
                address: Address::Pci {
                    segment: 0,
                    bus: 0,
                    device: (sid >> 3) as u8,
                    function: (sid & 7) as u8,
                    express: false,
                    bridge: false,
                },
                identity: bus.clone(),
                parent: None,
                needs: 0,
                aliases: 0,
                reset: 0,
                irq_scope: 0,
                ownership: Ownership::Independent,
                provider_trusted: true,
            }],
        )
        .map_err(|_| REFUSED)?;
    let s = manager.discover(slot).map_err(|_| REFUSED)?;
    let cap = manager
        .open_link(Binding {
            owner: 2,
            peer: 1,
            role: Role::Catalog,
            group: Some(s.attachment),
        })
        .map_err(|_| REFUSED)?;
    let target = Target {
        revision: epoch,
        attachment: "dev-007",
        bus,
        class: "ethernet",
        isa: "aarch64",
        os: [0, 20, 0],
        abi: 1,
        profile: Some(Sha256::digest(object!("profil.bin")).into()),
        features: &["courtier"],
        caps: if config(5) == 6 {
            &["rx"]
        } else {
            &["tx", "rx"]
        },
        dma: &[
            "descripteurs:r",
            "disponibles:r",
            "utilises:w",
            "tx:r",
            "rx:w",
        ],
        mmio: &[],
        irq: &[],
        allow_class_match: false,
        limits: Limits {
            private_pages: 96,
            roots: 1,
            handles: 3,
            boxes: 0,
            pending: 8,
            dma_pages: 22,
            queues: 2,
            entries: 16,
            irq_routes: 0,
        },
    };
    checkpoint(3, epoch)?;
    let mut acquitte = false;
    for step in 0..1431 {
        let msg = receive_for(config(0), 2500)?;
        if msg.kind != 1 || msg.tag != epoch || msg.len > 128 {
            return Err(REFUSED);
        }
        let bytes = &msg.payload[..msg.len as usize];
        let wire = Request::decode(bytes).map_err(|_| REFUSED)?;
        let permis = match step {
            0 => wire.op == Op::Hello,
            1 | 2 => wire.op == Op::Resolve,
            _ => matches!(wire.op, Op::ReadPlan | Op::Ack),
        };
        if !permis || wire.epoch != epoch {
            return Err(REFUSED);
        }
        // L’époque native est provisionnée par ouverture. L’objet 4C est local au démarrage du gestionnaire.
        let mut local = Request {
            epoch: cap.epoch(),
            ..wire
        };
        let mut buf = [0; 128];
        if local.op == Op::Ack {
            if wire.body.a != epoch {
                return Err(REFUSED);
            }
            local.body.a = cap.epoch();
        }
        let n = local.encode(&mut buf).map_err(|_| REFUSED)?;
        let reply = manager
            .serve(
                cap,
                &buf[..n],
                Some(Selection {
                    target: &target,
                    pin: None,
                    approved_members: slot.bit(),
                }),
            )
            .map_err(|_| REFUSED)?;
        send_for(config(1), epoch, &reply.encode(wire), 2500)?;
        if reply.status != Status::Confirmed {
            return Err(REFUSED);
        }
        if wire.op == Op::Ack {
            acquitte = true;
            break;
        }
    }
    if !acquitte {
        return Err(REFUSED);
    }
    let plan = manager.plan(slot).ok_or(REFUSED)?;
    let hw = plan.hardware();
    let q = plan.quotas();
    let exact = |have: &[alloc::string::String], need: &[&str]| {
        have.len() == need.len() && need.iter().all(|v| have.iter().any(|x| x == v))
    };
    if plan.revisions() != (1, epoch)
        || (
            hw.dma_pages,
            hw.queues,
            hw.entries_per_queue,
            hw.max_irq_routes,
        ) != (22, 2, 16, 0)
        || !hw.mmio.is_empty()
        || !hw.irq.is_empty()
        || !exact(&hw.dma, target.dma)
        || !exact(plan.capabilities(), target.caps)
        || (
            q.private_pages,
            q.ipc_roots_created,
            q.ipc_handles,
            q.ipc_boxes_created,
            q.pending_operations,
        ) != (96, 1, 3, 0, 8)
    {
        return Err(REFUSED);
    }
    for (i, w) in plan.image_digest().chunks_exact(8).enumerate() {
        mark(24 + i, u64::from_le_bytes(w.try_into().unwrap()));
    }
    for (offset, digest) in [
        (36, manager.plan_seal(slot).ok_or(REFUSED)?),
        (40, plan.package_digest()),
    ] {
        for (i, w) in digest.chunks_exact(8).enumerate() {
            mark(offset + i, u64::from_le_bytes(w.try_into().unwrap()));
        }
    }
    mark(44, 1); // révision du catalogue injecté
    mark(28, epoch);
    mark(29, plan.revisions().0);
    let (index, documents) = catalogue.memory_bytes();
    mark(30, index as u64);
    mark(31, documents as u64);
    mark(32, manager.inventory().bytes() as u64);
    mark(33, manager.registry_bytes() as u64);
    mark(34, manager.plan_slots_bytes() as u64);
    mark(35, manager.ipc_bytes().1 as u64);
    let taille_plan = plan.memory_bytes();
    mark(45, policy_bytes as u64);
    mark(46, taille_plan as u64);
    for (i, x) in mesures_tas::bilan().ok_or(REFUSED)?.into_iter().enumerate() {
        mark(47 + i, x as u64);
    }
    checkpoint(4, epoch)?;
    while config(19) != epoch {
        call(THREAD_YIELD, 0, 0, 0);
    }
    manager.plan_seal(slot).ok_or(REFUSED)
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    exit(match gate().and_then(lancement::conduire) {
        Ok(()) => 0,
        Err(e) => e,
    })
}
