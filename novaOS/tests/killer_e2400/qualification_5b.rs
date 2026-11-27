extern crate alloc;
#[allow(dead_code)]
#[path = "../../user/killer-e2400/adaptateur.rs"]
mod adaptateur;
#[allow(dead_code)]
#[path = "../../user/killer-e2400/boucle.rs"]
mod boucle;
#[path = "../../user/killer-e2400/courtier_simule.rs"]
mod courtier_simule;
#[allow(dead_code)]
#[path = "../../user/killer-e2400/profil.rs"]
mod profil;
mod scenario_5b;
mod support;
use novaos_gestionnaire_sim::{
    Error,
    catalogue::{Blob, Catalog},
    e2400::Permit,
    lifecycle::State,
    manager::Manager,
    resources::Use,
};
use scenario_5b::{node, selected};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use support::*;
thread_local! {static FAIL_LEDGER:Cell<bool>=const{Cell::new(false)};}
struct Allocation;
#[global_allocator]
static HEAP: Allocation = Allocation;
unsafe impl GlobalAlloc for Allocation {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        if l.size() == 777 && FAIL_LEDGER.with(|v| v.replace(false)) {
            core::ptr::null_mut()
        } else {
            unsafe { System.alloc(l) }
        }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
#[test]
fn courtier_cycle_complet_et_quarantaine_partielle() {
    let m = scenario_5b::run(|| 0).unwrap();
    assert_eq!(
        (m.register_heap, m.reservation_and_ack, m.replay_bytes),
        (777, 304, 152)
    );
    println!(
        "courtier_tete={} courtier_tas={} registre_tas={} reserve_ack_octets={} rejeu_octets={} table_plan={} permis={}",
        m.broker_header,
        m.broker_heap,
        m.register_heap,
        m.reservation_and_ack,
        m.replay_bytes,
        m.plan_table,
        m.permit_size
    );
}
#[test]
fn refus_avant_attribution_et_echec_allocation() {
    let mut p = politique(false, 3);
    p.add_resource(RECU, PROFIL, SIG_RECU).unwrap();
    let mut c = Catalog::new(&p);
    c.admit(
        MANIFESTE,
        SIG,
        &[Blob {
            isa: "x86_64",
            bytes: PKG,
        }],
    )
    .unwrap();
    for cas in 0..11 {
        let cap = if cas == 1 {
            Use {
                dma: 258,
                ..Use::LAB
            }
        } else {
            Use::LAB
        };
        let mut m = Manager::new(33, &c, 1, cap, Use::default(), Some(37), 0).unwrap();
        let mut disk = node(4, true);
        if cas == 7 {
            disk.reset = node(7, false).reset;
        }
        if cas == 8 {
            disk.irq_scope = node(7, false).irq_scope;
        }
        let mut sibling = node(6, false);
        if cas == 10 {
            sibling.provider_trusted = false;
        }
        m.refresh(1, true, [node(7, false), disk, sibling]).unwrap();
        let (s, mut topo) = selected(&mut m).unwrap();
        match cas {
            0 => FAIL_LEDGER.with(|v| v.set(true)),
            2 => topo.firmware_released = false,
            3 => topo.aliases_resolved = false,
            4 => topo.iommu_scopes_resolved = false,
            5 => topo.resets_and_irq_isolated = false,
            6 => topo.revision += 1,
            9 => m.nvme_lease(Some(475)).unwrap(),
            _ => {}
        }
        let usage = m.usage().unwrap();
        let expected = match cas {
            0 => Error::Memory,
            1 => Error::Full,
            2..=5 | 10 => Error::Incomplete,
            6 => Error::Stale,
            _ => Error::Denied,
        };
        let attempt = Permit::install(&mut m, s, topo, 15, 1, 88);
        let (err, permit) = match attempt {
            Ok(p) => (m.reserve(s, 15, 1, 88).err(), Some(p)),
            Err(err) => (Some(err), None),
        };
        assert_eq!(err, Some(expected), "cas {cas}");
        assert_eq!(m.usage().unwrap(), usage);
        assert_eq!(m.dma_registry_heap(), 0);
        let g = m.group(s.attachment.slot).unwrap();
        assert_eq!(g.state(), State::Verified);
        assert_eq!(g.stamp(), s);
        assert!(g.lease().is_none());
        if cas == 0 {
            assert!(!FAIL_LEDGER.with(Cell::get));
            m.reserve(s, 15, 1, 88).unwrap();
            let b = permit.unwrap().bind(&m).unwrap();
            assert_eq!(b.lease().generation, 1);
        }
    }
}

#[cfg(target_os = "macos")]
#[test]
fn cpu_du_lot_de_264_tx_rx() {
    let clock = || {
        let mut t = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // Sûreté : Darwin écrit dans un timespec local valide.
        assert_eq!(
            unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) },
            0
        );
        t.tv_sec as u64 * 1_000_000_000 + t.tv_nsec as u64
    };
    let mut samples = [0u64; 128];
    for sample in &mut samples {
        *sample = scenario_5b::run(clock).unwrap().transfer_cpu_ns;
    }
    samples.sort_unstable();
    println!(
        "courtier_264_allers_retours_cpu_ns p50={} p99={} series={}",
        samples[63],
        samples[126],
        samples.len()
    );
}

#[test]
fn identification_phy_du_banc_exige_deux_mots_valides_et_identiques() {
    use courtier_simule::Phy;
    let expected = Phy {
        oui_msb: 0x1234,
        model_revision: 0xb001,
    };
    expected.verify(expected).unwrap();
    for word in [0, 0xffff] {
        let bad = Phy {
            oui_msb: word,
            ..expected
        };
        assert_eq!(bad.verify(bad), Err(Error::Denied));
        let bad = Phy {
            model_revision: word,
            ..expected
        };
        assert_eq!(bad.verify(bad), Err(Error::Denied));
    }
    let other = Phy {
        model_revision: 0xb002,
        ..expected
    };
    assert_eq!(other.verify(expected), Err(Error::Denied));
}
