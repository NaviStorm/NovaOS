//! Exécutable de test isolé. Le refus d'une demande réelle concerne uniquement
//! le thread qui vient de lire le bloc ciblé. Les autres demandes vont à System.
use novafs::codec::{Anchor, Manifest};
use novafs::device::SimDevice;
use novafs::tree::{Family, Record};
use novafs::{Block, CheckIssue, Device, Engine, Error, Ids, PairStatus, Result, check_image};
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::ptr;

thread_local! {
    // TLS à initialisation constante, sans allocation ni destructeur.
    static DENY_ONCE: Cell<usize> = const { Cell::new(0) };
}

fn denied(size: usize) -> bool {
    DENY_ONCE
        .try_with(|request| {
            if size != 0 && request.get() == size {
                request.set(0);
                true
            } else {
                false
            }
        })
        .unwrap_or(false)
}

struct OneFailure;
#[global_allocator]
static ALLOCATOR: OneFailure = OneFailure;

// SAFETY: tous les pointeurs valides proviennent de System. Un refus renvoie
// null sans toucher à une allocation existante. Les autres opérations lui
// transmettent exactement la disposition et le pointeur fournis par l'appelant.
unsafe impl GlobalAlloc for OneFailure {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if denied(layout.size()) {
            return ptr::null_mut();
        }
        // SAFETY: même contrat GlobalAlloc et même layout.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        // SAFETY: pointeur vivant alloué par System sous ce layout.
        unsafe { System.dealloc(p, layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        if denied(layout.size()) {
            return ptr::null_mut();
        }
        // SAFETY: System reçoit la disposition originale et assure la mise à zéro.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, p: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if denied(new_size) {
            return ptr::null_mut();
        }
        // SAFETY: le bloc de System et sa disposition sont inchangés.
        unsafe { System.realloc(p, layout, new_size) }
    }
}

struct ArmAt<'a> {
    disk: &'a SimDevice,
    block: u64,
    bytes: usize,
    armed: Cell<bool>,
}
impl Device for ArmAt<'_> {
    fn blocks(&self) -> u64 {
        self.disk.blocks()
    }
    fn read(&self, block: u64) -> Result<Block> {
        let data = self.disk.read(block)?;
        if block == self.block && !self.armed.replace(true) {
            DENY_ONCE.with(|want| want.set(self.bytes));
        }
        Ok(data)
    }
    fn write(&mut self, _: u64, _: &Block) -> Result<()> {
        panic!("écriture pendant check")
    }
    fn flush(&mut self) -> Result<()> {
        panic!("flush pendant check")
    }
}

#[test]
fn refus_reels_bitmap_table_cle_et_valeur_puis_reprise() -> Result<()> {
    const BLOCKS: u64 = 513;
    let ids = Ids {
        fs: [0x29; 16],
        lv: [0xeb; 16],
    };
    let name = "n".repeat(201);
    for sector in [512, 4096] {
        let mut fs = Engine::format(SimDevice::new(BLOCKS, sector)?, ids)?;
        fs.create(&format!("/{name}"))?;
        let disk = fs.unmount()?;
        let control = disk.control();
        let before = control.snapshot().bytes()?;
        let anchor = Anchor::decode(&disk.read(0)?, 0)?;
        let m = Manifest::decode(&disk.read(anchor.manifest.block)?, ids, anchor.manifest)?;
        let root = m.roots[Family::Objects.index()].block;
        // L'ancre 0 contient la racine et le nouvel objet. La table de deux
        // records, les clés u64 et la valeur de longueur 28+nom sont distinctes.
        for (after, bytes, location) in [
            (
                anchor.manifest.block,
                BLOCKS.div_ceil(64) as usize * size_of::<u64>(),
                None,
            ),
            (root, 2 * size_of::<Record>(), Some(root)),
            (root, 8, Some(root)),
            (root, 28 + name.len(), Some(root)),
        ] {
            let gated = ArmAt {
                disk: &disk,
                block: after,
                bytes,
                armed: Cell::new(false),
            };
            let rejected = check_image(&gated, ids);
            let remaining = DENY_ONCE.with(|want| want.replace(0));
            assert!(
                gated.armed.get() && remaining == 0,
                "refus de {bytes} octets non consommé"
            );
            assert!(matches!(&rejected.slots[0].issue,Some(CheckIssue{
                error:Error::NoMemory,block,..}) if *block==location));
            assert!(rejected.slots[1].issue.is_none());
            assert!(matches!(rejected.pair, PairStatus::Unavailable));
            let retry = check_image(&disk, ids);
            assert!(matches!(retry.pair, PairStatus::Verified { slot: 0 }));
            assert_eq!(retry.slots[0].anchor.unwrap(), anchor);
            assert_eq!(control.snapshot().bytes()?, before);
        }
    }
    Ok(())
}
