use crate::{
    hal::memory::{Access, VirtualAddress},
    heap,
    memory::{self, PAGE},
};

const BASE: usize = 0xffff_f020_0000_0000;
const BYTES: usize = 64 * 1024;
const PAGE_BYTES: usize = PAGE as usize;
const MARK: u64 = 0xaf75_5a98_b66b_d213;

pub(super) struct Stack {
    pub low: usize,
    pub top: usize,
}
impl Stack {
    pub fn allocate(slot: usize) -> memory::Result<Self> {
        assert!(slot < super::MAX);
        let low = BASE + slot * (BYTES + 2 * PAGE_BYTES) + PAGE_BYTES;
        heap::with_pages(|m| {
            for offset in (0..BYTES).step_by(PAGE_BYTES) {
                if let Err(e) = m.allocate_page(VirtualAddress(low + offset), Access::ReadWrite) {
                    for undo in (0..offset).step_by(PAGE_BYTES).rev() {
                        // Sûreté : pages privées de cette réservation encore non publiée.
                        unsafe {
                            m.release_page(VirtualAddress(low + undo))?;
                        }
                    }
                    return Err(e);
                }
            }
            Ok(())
        })?;
        // Sûreté : premier mot de la pile fraîchement mappée, réservé au témoin.
        unsafe { (low as *mut u64).write_volatile(MARK) };
        Ok(Self {
            low,
            top: low + BYTES,
        })
    }

    pub fn check(&self, sp: usize) {
        // Sûreté : la pile du slot reste mappée jusqu’à sa collecte hors interruption.
        let mark = unsafe { (self.low as *const u64).read_volatile() };
        assert!(
            sp >= self.low + 512 && sp <= self.top && mark == MARK,
            "limite de pile noyau"
        );
    }

    pub fn release(self) {
        heap::with_pages(|m| {
            for va in (self.low..self.top).step_by(PAGE_BYTES) {
                // Sûreté : seul un thread terminé, déjà commuté, cède sa pile.
                unsafe {
                    m.release_page(VirtualAddress(va))?;
                }
            }
            Ok(())
        })
        .expect("restitution de pile");
    }
}
