//! Lecture paginée du plan sans deuxième copie. Les entiers sont little-endian.
use crate::{Error, Result, catalogue::Plan};
const CHUNK: usize = 23;
struct Window {
    start: usize,
    total: usize,
    out: [u8; 32],
}
impl Sink for Window {
    fn bytes(&mut self, data: &[u8]) {
        let base = self.total;
        self.total += data.len();
        let lo = base.max(self.start);
        let hi = self.total.min(self.start + CHUNK);
        if lo < hi {
            self.out[9 + lo - self.start..9 + hi - self.start]
                .copy_from_slice(&data[lo - base..hi - base]);
        }
    }
}
trait Sink {
    fn bytes(&mut self, data: &[u8]);
    fn text(&mut self, s: &str) {
        self.bytes(&(s.len() as u16).to_le_bytes());
        self.bytes(s.as_bytes());
    }
    fn names(&mut self, ss: &[alloc::string::String]) {
        self.bytes(&[ss.len() as u8]);
        for s in ss {
            self.text(s)
        }
    }
    fn version(&mut self, v: [u32; 3]) {
        for part in v {
            self.bytes(&part.to_le_bytes())
        }
    }
}
pub(crate) fn read(plan: &Plan, cursor: u64) -> Result<[u8; 32]> {
    if cursor == 0 {
        return Ok(plan.manifest_digest());
    }
    let start = usize::try_from(cursor - 1).map_err(|_| Error::Format)?;
    if start >= 32768 || !start.is_multiple_of(CHUNK) {
        return Err(Error::Format);
    }
    let mut w = Window {
        start,
        total: 0,
        out: [0; 32],
    };
    serialize(plan, &mut w);
    if start >= w.total {
        return Err(Error::Stale);
    }
    let len = (w.total - start).min(CHUNK);
    let next = if start + len < w.total {
        cursor + len as u64
    } else {
        0
    };
    w.out[..8].copy_from_slice(&next.to_le_bytes());
    w.out[8] = len as u8;
    Ok(w.out)
}

fn serialize(plan: &Plan, w: &mut impl Sink) {
    w.bytes(b"NPL1");
    w.text(plan.driver());
    w.version(plan.version());
    w.bytes(&plan.manifest_digest());
    w.bytes(&plan.image_digest());
    w.bytes(&plan.package_digest());
    let (policy, inventory) = plan.revisions();
    w.bytes(&policy.to_le_bytes());
    w.bytes(&inventory.to_le_bytes());
    w.text(plan.attachment());
    w.text(plan.architecture());
    w.names(plan.capabilities());
    let h = plan.hardware();
    w.names(&h.dma);
    w.names(&h.mmio);
    w.names(&h.irq);
    w.bytes(&h.dma_pages.to_le_bytes());
    for v in [h.max_irq_routes, h.queues, h.entries_per_queue] {
        w.bytes(&v.to_le_bytes());
    }
    let q = plan.quotas();
    w.bytes(&q.private_pages.to_le_bytes());
    w.bytes(&[q.ipc_roots_created, q.ipc_boxes_created, q.ipc_handles]);
    w.bytes(&q.pending_operations.to_le_bytes());
    w.bytes(&[plan.dependencies().len() as u8]);
    for b in plan.dependencies() {
        w.text(&b.id);
        w.bytes(&[match b.kind {
            crate::catalogue::Kind::SystemProfile => 1,
            crate::catalogue::Kind::Controller => 2,
            crate::catalogue::Kind::Service => 3,
        }]);
        w.version(b.version);
        w.bytes(&b.sha256);
        w.bytes(&b.record_digest);
    }
}
impl Sink for sha2::Sha256 {
    fn bytes(&mut self, data: &[u8]) {
        sha2::Digest::update(self, data)
    }
}
pub(crate) fn seal(plan: &Plan) -> [u8; 32] {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    serialize(plan, &mut h);
    h.finalize().into()
}
