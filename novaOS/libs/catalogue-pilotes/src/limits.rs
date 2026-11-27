use crate::{Hardware, Quotas};

/// Réserves restantes pour le groupe et son propriétaire, fournies hors ligne.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub private_pages: u32,
    pub roots: u8,
    pub handles: u8,
    pub boxes: u8,
    pub pending: u16,
    pub dma_pages: u32,
    pub queues: u16,
    pub entries: u16,
    pub irq_routes: u16,
}
impl Limits {
    pub(crate) fn fits(&self, q: &Quotas, h: &Hardware) -> bool {
        q.private_pages <= self.private_pages
            && q.ipc_roots_created <= self.roots.min(4)
            && q.ipc_handles <= self.handles.min(32)
            && q.ipc_boxes_created <= self.boxes.min(8)
            && q.pending_operations <= self.pending
            && h.dma_pages <= self.dma_pages
            && h.queues <= self.queues
            && h.entries_per_queue <= self.entries
            && h.max_irq_routes <= self.irq_routes
    }
}
