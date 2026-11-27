use crate::{Error, Result, catalogue::Plan};
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Use {
    pub pages: u32,
    pub dma: u32,
    pub boxes: u16,
    pub endpoints: u16,
    pub nodes: u16,
    pub waits: u16,
    pub processes: u16,
    pub receipts: u16,
}
impl Use {
    pub fn checked_add(self, other: Self) -> Result<Self> {
        Ok(Self {
            pages: self.pages.checked_add(other.pages).ok_or(Error::Full)?,
            dma: self.dma.checked_add(other.dma).ok_or(Error::Full)?,
            boxes: self.boxes.checked_add(other.boxes).ok_or(Error::Full)?,
            endpoints: self
                .endpoints
                .checked_add(other.endpoints)
                .ok_or(Error::Full)?,
            nodes: self.nodes.checked_add(other.nodes).ok_or(Error::Full)?,
            waits: self.waits.checked_add(other.waits).ok_or(Error::Full)?,
            processes: self
                .processes
                .checked_add(other.processes)
                .ok_or(Error::Full)?,
            receipts: self
                .receipts
                .checked_add(other.receipts)
                .ok_or(Error::Full)?,
        })
    }
    pub fn fits(self, cap: Self) -> bool {
        self.pages <= cap.pages
            && self.dma <= cap.dma
            && self.boxes <= cap.boxes
            && self.endpoints <= cap.endpoints
            && self.nodes <= cap.nodes
            && self.waits <= cap.waits
            && self.processes <= cap.processes
            && self.receipts <= cap.receipts
    }
    pub const LAB: Self = Self {
        pages: 8192,
        dma: 4096,
        boxes: 8,
        endpoints: 64,
        nodes: 256,
        waits: 32,
        processes: 32,
        receipts: 64,
    };
    pub fn plan(p: &Plan) -> Result<Self> {
        let q = p.quotas();
        let dma = p.hardware().dma_pages;
        if dma > crate::dma::MAX_FRAMES as u32 || q.ipc_roots_created > 4 || q.ipc_handles > 32 {
            return Err(Error::Full);
        }
        Ok(Self {
            pages: q.private_pages.checked_add(1).ok_or(Error::Full)?,
            dma,
            boxes: q.ipc_boxes_created.into(),
            endpoints: q.ipc_roots_created.into(),
            nodes: q.ipc_handles.into(),
            waits: 1,
            processes: 1,
            receipts: 1,
        })
    }
}
