//! Canaux privés du profil NVMe. Les droits de chaque racine sont réduits
//! sous la garde du lanceur avant la première instruction de ses titulaires.
use super::*;
use crate::sched::process::nvme::{Peers, profile, wire};

impl Domain {
    fn nvme_link(&mut self, sender: Pid, receiver: Pid) -> Res<(u64, u64)> {
        let root = self.create(receiver)?;
        let send = self.derive(receiver, root, sender, SEND.into())?;
        let n = self.lookup(receiver, root, ALL)?;
        self.nodes.get_mut(n.id.slot()).rights = RECEIVE;
        Ok((send, root))
    }

    /// La racine privée consomme le quota de l'autorité supérieure. Seul le
    /// service bloc reçoit SEND ; la racine sans droits permet le retrait par
    /// le superviseur et la mort de son propriétaire ferme toute la délégation.
    fn nvme_grant(&mut self, owner: Pid, holder: Pid) -> Res<u64> {
        let key = self.create(owner)?;
        let grant = self.derive(owner, key, holder, SEND.into())?;
        let root = self.lookup(owner, key, ALL)?;
        self.nodes.get_mut(root.id.slot()).rights = 0;
        Ok(grant)
    }
    pub(in crate::sched::process) fn nvme_withdraw(&mut self, holder: Pid, key: u64) {
        if let Ok(n) = self.lookup(holder, key, SEND) {
            self.close(n);
            self.signal_heads();
            self.collect();
        }
    }
    pub(in crate::sched::process) fn nvme_live(&self, pid: Pid, key: u64, rights: u32) -> bool {
        self.lookup(pid, key, rights)
            .is_ok_and(|n| n.rights == rights)
    }

    /// Observer sans consommer la prochaine demande du titulaire dans le
    /// témoin de révocation ; seul nvme_request effectue le retrait de file.
    pub(in crate::sched::process) fn nvme_front(
        &self,
        pid: Pid,
        port: u64,
    ) -> Res<Option<(u64, u64)>> {
        let n = self.lookup(pid, port, SEND)?;
        let queue = self.ends.get(n.endpoint.slot());
        if queue.len == 0 {
            return Ok(None);
        }
        let msg = &queue.deposits[queue.head].msg;
        if msg.len as usize != wire::REQUEST_SIZE {
            return Err(BAD_ARGUMENT);
        }
        Ok(Some((msg.tag, wire::word(&msg.bytes, 2))))
    }

    pub(in crate::sched::process) fn nvme_request(
        &mut self,
        pid: Pid,
        port: u64,
    ) -> Res<Option<(u64, [u64; 5])>> {
        if !self.nvme_live(pid, port, SEND) {
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
) -> Res<([Child; 3], Peers, Option<Child>)> {
    let _gate = Guard::new();
    let mut children: [Option<Child>; 3] = [None, None, None];
    let mut application = None;
    let rw = mode >= profile::INIT;
    let images = if rw {
        [
            include_bytes!(concat!(env!("OUT_DIR"), "/nvme_rw_driver.bin")).as_slice(),
            include_bytes!(concat!(env!("OUT_DIR"), "/nvme_rw_block.bin")).as_slice(),
            include_bytes!(concat!(env!("OUT_DIR"), "/storage-media.bin")).as_slice(),
        ]
    } else {
        images
    };
    let staged = (|| {
        for (slot, image) in children.iter_mut().zip(images) {
            *slot = Some(create_image(image, &[], None, None).map_err(launch_error)?);
        }
        if rw {
            let app_image = include_bytes!(concat!(env!("OUT_DIR"), "/storage-fileclient.bin")).as_slice();
            #[cfg(feature="net-q35-duo")]
            let app_image = if mode == profile::MEASURE {
                include_bytes!(concat!(env!("OUT_DIR"), "/storage-net_measure.bin")).as_slice()
            } else {app_image};
            application = Some(
                create_image(
                    app_image,
                    &[],
                    None,
                    None,
                )
                .map_err(launch_error)?,
            );
        }
        let app = application.as_ref().map(|c| c.0);
        let pids = children.each_ref().map(|c| c.as_ref().unwrap().0);
        let [driver, block, client] = pids;
        with(|s| {
            let mut cfg = [[0u64; 11]; 3];
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
            let mut media_words = [0; 11];
            let mut app_words = [0; 4];
            let mut app_notice = 0;
            let mut file_links = [0; 5];
            if let Some(app) = app {
                if profile::writable(mode) {
                    cfg[1][wire::WRITE_CAP] = s.ipc.nvme_grant(client, block)?;
                    cfg[1][wire::FLUSH_CAP] = s.ipc.nvme_grant(app, block)?;
                }
                let lane = s.ipc.create(client)?;
                let root = s.ipc.derive(client, lane, block, (SEND | RECEIVE).into())?;
                s.ipc.bind_lane(client, lane, block, root)?;
                let node = s.ipc.lookup(client, lane, ALL)?;
                s.ipc.nodes.get_mut(node.id.slot()).rights = SEND | RECEIVE;
                cfg[1][wire::PAGE_LANE] = root;
                let (incoming, request) = s.ipc.nvme_link(app, client)?;
                let (response, outgoing) = s.ipc.nvme_link(client, app)?;
                s.ipc.locals.get_mut(client.slot).bootstrap = cfg[2][wire::FRONT_REQUEST];
                s.ipc.locals.get_mut(app.slot).bootstrap = incoming;
                media_words[0] = cfg[2][wire::FRONT_REPLY];
                media_words[1] = profile::file_mode(mode);
                media_words[2] = profile::cut(mode);
                media_words[4] = request;
                media_words[5] = response;
                media_words[10] = lane;
                file_links = [incoming, outgoing, request, response, lane];
                if mode == profile::CLIENT_DEATH {
                    app_notice = s.ipc.lab_receiver(app)?;
                }
                app_words = [
                    outgoing,
                    if matches!(mode, profile::CLIENT_DEATH | profile::RACE) {
                        mode
                    } else {
                        #[cfg(feature="net-q35-duo")]
                        if mode == profile::MEASURE {mode} else {profile::file_mode(mode)}
                        #[cfg(not(feature="net-q35-duo"))]
                        profile::file_mode(mode)
                    },
                    profile::cut(mode),
                    app_notice,
                ];
            }
            heap::with_pages(|m| {
                for (pid, row) in pids.iter().zip(&cfg) {
                    let space = s.processes.get(*pid).space.as_ref().unwrap();
                    for (i, v) in row.iter().enumerate() {
                        space.write_data(m, i, *v);
                    }
                }
                if let Some(app) = app {
                    for (pid, words) in [(client, &media_words[..]), (app, &app_words[..])] {
                        let space = s.processes.get(pid).space.as_ref().unwrap();
                        for (i, &word) in words.iter().enumerate() {
                            space.write_data(m, i, word);
                        }
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
            Ok(Peers {
                pids,
                cfg,
                app,
                app_notice,
                file_links,
            })
        })
    })();
    match staged {
        Ok(peers) => Ok((children.map(Option::unwrap), peers, application)),
        Err(err) => {
            for child in children.into_iter().flatten().chain(application) {
                retire_unpublished(child);
            }
            Err(err)
        }
    }
}
