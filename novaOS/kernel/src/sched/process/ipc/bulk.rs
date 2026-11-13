//! Une page superviseur par paire explicitement reliée. La boîte ne sert pas de file IPC.
use super::*;
pub(super) type Buffers = Pool<[u8; BULK_MAX], 8, 84, 1>;

#[derive(Clone, Copy)]
pub(super) struct Lane {
    slot: usize,
    parties: [u64; 2],
    pub(super) nodes: [Reference; 2],
    handles: [u64; 2],
    owner: usize,
    tag: u64,
    len: usize,
}

impl Domain {
    pub(super) fn bind_lane(&mut self, a: Pid, ka: u64, b: Pid, kb: u64) -> Res<()> {
        let root = self.lookup(a, ka, MANAGE)?;
        let peer = self.lookup(b, kb, RECEIVE)?;
        let ep = root.endpoint;
        if a.number == b.number || peer.endpoint != ep || self.ends.get(ep.slot()).bulk.is_some() {
            return Err(DENIED);
        }
        let slot = self.pages_bulk.vacant()?;
        self.pages_bulk.insert(slot, [0; BULK_MAX])?;
        self.ends.get_mut(ep.slot()).bulk = Some(Lane {
            slot,
            nodes: [root.id, peer.id],
            parties: [a.number, b.number],
            handles: [ka, kb],
            owner: 0,
            tag: 0,
            len: 0,
        });
        Ok(())
    }

    pub(super) fn release_lane(&mut self, ep: Reference) {
        if let Some(lane) = self.ends.get_mut(ep.slot()).bulk.take() {
            self.pages_bulk.remove(lane.slot);
        }
    }
}

pub(in crate::sched::process) fn transfer(
    op: u64,
    key: u64,
    descriptor: u64,
    size: u64,
) -> Res<u64> {
    if size != size_of::<BulkV1>() as u64 || descriptor % 8 != 0 {
        return Err(BAD_ARGUMENT);
    }
    with(|s| {
        let pid = s.thread(s.current).owner.unwrap();
        let ipc = &mut s.ipc;
        let put = op == IPC_PUT;
        let node = ipc.lookup(pid, key, if put { SEND } else { RECEIVE })?;
        let lane = ipc
            .ends
            .get_mut(node.endpoint.slot())
            .bulk
            .as_mut()
            .ok_or(DENIED)?;
        let side = lane
            .parties
            .iter()
            .position(|&p| p == pid.number)
            .ok_or(DENIED)?;
        if lane.handles[side] != key {
            return Err(DENIED);
        }
        let space = s.processes.get(pid).space.as_ref().unwrap();
        heap::with_pages(|m| {
            let mut desc = [0; 32];
            if !space.copy_from_user(m, descriptor, &mut desc) {
                return Ok(Err(BAD_ADDRESS));
            }
            let word = |at: usize| u64::from_le_bytes(desc[at..at + 8].try_into().unwrap());
            let tag = word(8);
            let addr = word(16);
            let len = word(24);
            if desc[..4] != VERSION.to_le_bytes()
                || desc[4..8] != 32u32.to_le_bytes()
                || tag == 0
                || !(1..=BULK_MAX as u64).contains(&len)
            {
                return Ok(Err(BAD_ARGUMENT));
            }
            let buf = &mut ipc.pages_bulk.get_mut(lane.slot)[..len as usize];
            if put {
                if lane.len != 0 {
                    return Ok(Err(WOULD_BLOCK));
                }
                if !space.copy_from_user(m, addr, buf) {
                    return Ok(Err(BAD_ADDRESS));
                }
                lane.owner = side;
                lane.tag = tag;
                lane.len = buf.len();
            } else {
                if side == lane.owner || lane.tag != tag || lane.len != buf.len() {
                    return Ok(Err(REFUSED));
                }
                if !space.copy_to_user(m, addr, buf) {
                    return Ok(Err(BAD_ADDRESS));
                }
                buf.fill(0);
                lane.len = 0;
            }
            Ok(Ok(len))
        })
        .unwrap()
    })
}
