//! File du pilote EDU. La racine SEND seule ne peut être déléguée. Le
//! superviseur la lit sous la même garde que la révocation de l'instance.
use super::*;

impl Domain {
    pub(in crate::sched::process) fn lab_sender(&mut self, pid: Pid) -> Res<u64> {
        let key = self.create(pid)?;
        let node = self.lookup(pid, key, ALL)?;
        self.nodes.get_mut(node.id.slot()).rights = SEND;
        Ok(key)
    }

    pub(in crate::sched::process) fn lab_close_sender(&mut self, pid: Pid, key: u64) {
        if let Ok(n) = self.lookup(pid, key, SEND)
            && n.rights == SEND
            && n.id == self.ends.get(n.endpoint.slot()).root
        {
            self.close(n);
            self.signal_heads();
            self.collect();
        }
    }

    pub(in crate::sched::process) fn lab_ack_queued(
        &self,
        pid: Pid,
        key: u64,
        tag: u64,
        epoch: u64,
        seq: u64,
    ) -> bool {
        let Ok(n) = self.lookup(pid, key, SEND) else {
            return false;
        };
        let e = self.ends.get(n.endpoint.slot());
        let m = e.deposits[e.head].msg;
        e.owner == pid.number
            && n.rights == SEND
            && e.root == n.id
            && e.len == 1
            && m.len == 16
            && m.tag == tag
            && m.bytes[..8] == epoch.to_le_bytes()
            && m.bytes[8..16] == seq.to_le_bytes()
    }
    pub(in crate::sched::process) fn lab_pop(
        &mut self,
        pid: Pid,
        key: u64,
    ) -> Res<Option<(u64, u64, u64)>> {
        let n = self.lookup(pid, key, SEND)?;
        let e = self.ends.get_mut(n.endpoint.slot());
        if n.rights != SEND || e.root != n.id || e.owner != pid.number {
            return Err(DENIED);
        }
        if e.len == 0 {
            return Ok(None);
        }
        let m = e.deposits[e.head].msg;
        e.deposits[e.head].msg = Message::EMPTY;
        e.head = (e.head + 1) % e.deposits.len();
        e.len -= 1;
        if m.len != 16 {
            return Err(BAD_ARGUMENT);
        }
        Ok(Some((
            m.tag,
            u64::from_le_bytes(m.bytes[..8].try_into().unwrap()),
            u64::from_le_bytes(m.bytes[8..16].try_into().unwrap()),
        )))
    }
}
