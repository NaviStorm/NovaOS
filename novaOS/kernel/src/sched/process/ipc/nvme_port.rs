//! Canaux privés du profil NVMe. Les droits de chaque racine sont réduits
//! sous la garde du lanceur avant la première instruction de ses titulaires.
use super::*;
use crate::sched::process::nvme::{Peers, wire};

impl Domain {
    fn nvme_link(&mut self, sender: Pid, receiver: Pid) -> Res<(u64, u64)> {
        let root = self.create(receiver)?;
        let send = self.derive(receiver, root, sender, SEND.into())?;
        let n = self.lookup(receiver, root, ALL)?;
        self.nodes.get_mut(n.id.slot()).rights = RECEIVE;
        Ok((send, root))
    }

    pub(in crate::sched::process) fn nvme_live(&self, pid: Pid, key: u64, rights: u32) -> bool {
        self.lookup(pid, key, rights)
            .is_ok_and(|n| n.rights == rights)
    }

    pub(in crate::sched::process) fn nvme_request(
        &mut self,
        pid: Pid,
        port: u64,
    ) -> Res<Option<(u64, [u64; 5])>> {
        if !self.lab_can_submit(pid, port) {
            return Err(DENIED);
        }
        let n = self.lookup(pid, port, SEND)?;
        let queue = self.ends.get_mut(n.endpoint.slot());
        if queue.len == 0 {
            return Ok(None);
        }
        let msg = queue.deposits[queue.head].msg;
        queue.deposits[queue.head].msg = Message::EMPTY;
        queue.head = (queue.head + 1) % queue.deposits.len();
        queue.len -= 1;
        if msg.len as usize != wire::REQUEST_SIZE {
            return Err(BAD_ARGUMENT);
        }
        Ok(Some((
            msg.tag,
            core::array::from_fn(|i| wire::word(&msg.bytes, i)),
        )))
    }
}

pub(in crate::sched::process) fn nvme_group(
    images: [&'static [u8]; 3],
    mode: u64,
) -> Res<([Child; 3], Peers)> {
    let _gate = Guard::new();
    let mut children: [Option<Child>; 3] = [None, None, None];
    let staged = (|| {
        for (slot, image) in children.iter_mut().zip(images) {
            *slot = Some(create_image(image, &[], None, None).map_err(launch_error)?);
        }
        let pids = children.each_ref().map(|c| c.as_ref().unwrap().0);
        let [driver, block, client] = pids;
        with(|s| {
            let mut cfg = [[0u64; 8]; 3];
            for row in &mut cfg {
                row[wire::EPOCH] = driver.number;
                row[wire::MODE] = mode;
            }
            for i in 0..2 {
                cfg[i][wire::BROKER_SEND] = s.ipc.lab_sender(pids[i])?;
                cfg[i][wire::BROKER_REPLY] = s.ipc.lab_receiver(pids[i])?;
            }
            (cfg[2][wire::FRONT_REQUEST], cfg[1][wire::FRONT_REQUEST]) =
                s.ipc.nvme_link(client, block)?;
            (cfg[1][wire::FRONT_REPLY], cfg[2][wire::FRONT_REPLY]) =
                s.ipc.nvme_link(block, client)?;
            (cfg[1][wire::DRIVER_REQUEST], cfg[0][wire::DRIVER_REQUEST]) =
                s.ipc.nvme_link(block, driver)?;
            (cfg[0][wire::DRIVER_REPLY], cfg[1][wire::DRIVER_REPLY]) =
                s.ipc.nvme_link(driver, block)?;
            heap::with_pages(|m| {
                for (pid, row) in pids.iter().zip(&cfg) {
                    let space = s.processes.get(*pid).space.as_ref().unwrap();
                    for (i, v) in row.iter().enumerate() {
                        space.write_data(m, i, *v);
                    }
                }
                Ok(())
            })
            .map_err(|_| NO_MEMORY)?;
            let last = crate::smp::count() - 1;
            for (pid, cpu) in [(driver, last), (block, 0), (client, last / 2)] {
                s.slots
                    .iter_mut()
                    .flatten()
                    .find(|t| t.owner == Some(pid))
                    .unwrap()
                    .cpu_mask = 1 << cpu;
            }
            Ok(Peers { pids, cfg })
        })
    })();
    match staged {
        Ok(peers) => Ok((children.map(Option::unwrap), peers)),
        Err(err) => {
            for child in children.into_iter().flatten() {
                retire_unpublished(child);
            }
            Err(err)
        }
    }
}
