//! Exercer directement le courtier avec une BAR et des anneaux synthétiques.
//! Le simulateur écrit seulement les données qu'un contrôleur aurait publiées.
#[path = "../kernel/src/hardware/nvme/mod.rs"]
mod nvme;
use nvme::{Command::*, Controller, Error, Memory};

#[repr(align(4096))]
struct Aligned<const N: usize>([u8; N]);

struct Model {
    bar: Box<Aligned<8192>>,
    pages: Box<[Aligned<4096>; 6]>,
    writable: bool,
    heads: [usize; 2],
    phases: [bool; 2],
}

fn u32_at(buf: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(buf[at..at + 4].try_into().unwrap())
}
fn put32(buf: &mut [u8], at: usize, value: u32) {
    buf[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn put64(buf: &mut [u8], at: usize, value: u64) {
    buf[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

impl Model {
    fn new() -> Self {
        let mut model = Self {
            bar: Box::new(Aligned([0; 8192])),
            writable: false,
            pages: Box::new(core::array::from_fn(|_| Aligned([0; 4096]))),
            heads: [0; 2],
            phases: [true; 2],
        };
        put64(&mut model.bar.0, 0, 0xff | (15 << 24) | (1 << 37));
        put32(&mut model.bar.0, 8, 0x10400);
        model
    }
    fn memory(&mut self) -> Memory {
        let mut cpu = self.pages.each_mut().map(|p| p.0.as_mut_ptr() as usize);
        let mut iova = [0x10000, 0x11000, 0x12000, 0x13000, 0x14000, 0x15000];
        if !self.writable {
            cpu[5] = 0;
            iova[5] = 0;
        }
        Memory {
            phase2_media: self.writable,
            epoch: 1,
            bar: self.bar.0.as_mut_ptr() as usize,
            len: 8192,
            cpu,
            iova,
        }
    }
    fn prepare(&mut self) -> Controller {
        unsafe { Controller::prepare(self.memory()).unwrap() }
    }
    fn start(&mut self) -> Controller {
        let mut c = self.prepare();
        c.enable(0).unwrap();
        assert!(!c.ready(10).unwrap());
        put32(&mut self.bar.0, 0x1c, 1);
        assert!(c.ready(20).unwrap());
        c
    }

    /// Reconstruire les champs de la CQ à partir du descripteur effectivement
    /// publié sur la SQ, au lieu de deviner le CID du courtier.
    fn complete(&mut self, q: usize, wrong_cid: bool, sc: u16) {
        let tail = u32_at(&self.bar.0, 0x1000 + 8 * q) as usize;
        let sq = &self.pages[2 * q].0;
        let slot = (tail + 15) % 16;
        let cid = ((u32_at(sq, 64 * slot) >> 16) as u16).wrapping_add(wrong_cid as u16);
        let buf = &mut self.pages[2 * q + 1].0;
        let pos = 16 * self.heads[q];
        put32(buf, pos, 0);
        put32(buf, pos + 8, (tail | q << 16) as u32);
        put32(
            buf,
            pos + 12,
            u32::from(cid) | u32::from(self.phases[q]) << 16 | u32::from(sc) << 17,
        );
        self.heads[q] = (self.heads[q] + 1) % 16;
        if self.heads[q] == 0 {
            self.phases[q] = !self.phases[q];
        }
    }
    fn opcode(&self, q: usize) -> u8 {
        let last = (u32_at(&self.bar.0, 0x1000 + 8 * q) as usize + 15) % 16;
        self.pages[2 * q].0[last * 64]
    }
    fn identify(&mut self, cmd: nvme::Command, sector_exp: u8) {
        let data = &mut self.pages[4].0;
        match cmd {
            IdentifyController => {
                data[0..2].copy_from_slice(&0x1b36u16.to_le_bytes());
                data[4..64].fill(b' ');
                let serial: &[u8] = if self.writable {
                    b"NOVA0190RW"
                } else {
                    b"NOVA0190READ"
                };
                data[4..4 + serial.len()].copy_from_slice(serial);
                data[24..38].copy_from_slice(b"QEMU NVMe Ctrl");
                put32(data, 80, 0x10400);
                data[512] = 0x66;
                data[513] = 0x44;
                put32(data, 516, 256);
            }
            ListNamespaces => put32(data, 0, 1),
            IdentifyNamespace => {
                put64(data, 0, 128);
                put64(data, 8, 128);
                put64(data, 16, 128);
                data[130] = sector_exp;
                data[120..128].copy_from_slice(b"NOVA0190");
            }
            _ => (),
        }
    }
    fn activate(&mut self, sector_exp: u8) -> Controller {
        let mut c = self.start();
        for (command, op) in [
            (IdentifyController, 6),
            (ListNamespaces, 6),
            (IdentifyNamespace, 6),
            (NumberQueues, 9),
            (CreateCq, 5),
            (CreateSq, 1),
        ] {
            c.submit(command, 30).unwrap();
            assert_eq!(self.opcode(0), op);
            assert!(c.poll(30).unwrap().is_none());
            self.identify(command, sector_exp);
            self.complete(0, false, 0);
            let done = c.poll(31).unwrap().unwrap();
            if command == IdentifyNamespace {
                assert_eq!(done.nsid, 1);
                assert_eq!(done.block_size, 1u32 << sector_exp);
                assert_eq!(done.sectors, 128);
                assert_eq!(&done.eui64, b"NOVA0190");
            }
        }
        c
    }
}

#[test]
fn inherited_queues_or_enabled_controller_refused_before_mutation() {
    for offset in [0x14, 0x1c, 0x24, 0x28, 0x30] {
        let mut model = Model::new();
        put32(&mut model.bar.0, offset, 1);
        let before = model.bar.0;
        assert!(matches!(
            unsafe { Controller::prepare(model.memory()) },
            Err(Error::Inherited)
        ));
        assert_eq!(&model.bar.0, &before);
    }
}

#[test]
fn alias_and_invalid_tail_doorbell_rejected() {
    let mut m = Model::new();
    let mut mem = m.memory();
    mem.iova[4] = mem.iova[1];
    assert!(matches!(
        unsafe { Controller::prepare(mem) },
        Err(Error::Memory)
    ));
    let mut mem = m.memory();
    mem.cpu[0] = mem.cpu[2];
    assert!(matches!(
        unsafe { Controller::prepare(mem) },
        Err(Error::Memory)
    ));
    put64(&mut m.bar.0, 0, 15 | 1 << 24 | 1 << 37 | 15 << 32);
    assert!(matches!(
        unsafe { Controller::prepare(m.memory()) },
        Err(Error::Unsupported)
    ));
}

#[test]
fn timeout_blocks_late_cqe_and_release() {
    let mut m = Model::new();
    let mut c = m.prepare();
    c.enable(123).unwrap();
    assert_eq!(c.ready(7623), Err(Error::Timeout));
    put32(&mut m.bar.0, 0x1c, 1);
    assert_eq!(c.ready(7624), Err(Error::State));
    assert!(c.drained().is_none());
    let mut m = Model::new();
    let mut c = m.start();
    c.submit(IdentifyController, 200).unwrap();
    assert!(matches!(c.poll(5200), Err(Error::Timeout)));
    m.identify(IdentifyController, 9);
    m.complete(0, false, 0);
    assert!(matches!(c.poll(5201), Err(Error::State)));
    assert!(c.drained().is_none());
}

#[test]
fn unknown_or_unsuccessful_completion_never_delivers() {
    for (wrong, status) in [(true, 0), (false, 2)] {
        let mut m = Model::new();
        let mut c = m.start();
        c.submit(IdentifyController, 30).unwrap();
        m.identify(IdentifyController, 9);
        m.complete(0, wrong, status);
        let e = c.poll(31).unwrap_err();
        assert_eq!(
            e,
            if wrong {
                Error::Completion
            } else {
                Error::Status(2)
            }
        );
        assert!(c.drained().is_none());
        assert!(c.read_data(0, &mut [0u8; 1]).is_err());
    }
}

#[test]
fn reject_wrong_serial_and_namespace_list() {
    for malformed in [IdentifyController, ListNamespaces] {
        let mut m = Model::new();
        let mut c = m.start();
        for cmd in [IdentifyController, ListNamespaces] {
            c.submit(cmd, 30).unwrap();
            m.identify(cmd, 9);
            if cmd == malformed {
                let at = if cmd == IdentifyController {
                    4
                } else {
                    4 + 100 * 4
                };
                m.pages[4].0[at] = 2;
            }
            m.complete(0, false, 0);
            if cmd == malformed {
                assert!(c.poll(31).is_err());
                break;
            }
            c.poll(31).unwrap();
        }
        assert!(c.drained().is_none());
    }
}

#[test]
fn reject_untrusted_namespace_or_format() {
    for offset in [26, 29, 30, 120, 128, 130] {
        let mut m = Model::new();
        let mut c = m.start();
        for cmd in [IdentifyController, ListNamespaces, IdentifyNamespace] {
            c.submit(cmd, 30).unwrap();
            m.identify(cmd, 9);
            if cmd == IdentifyNamespace {
                m.pages[4].0[offset] = 255;
            }
            m.complete(0, false, 0);
            if cmd == IdentifyNamespace {
                assert!(c.poll(31).is_err());
            } else {
                c.poll(31).unwrap();
            }
        }
        assert!(c.drained().is_none());
    }
}

#[test]
fn read_bounds_wrap_phase_and_exact_result_both_geometries() {
    for exponent in [9, 12] {
        let mut m = Model::new();
        let mut c = m.activate(exponent);
        let sector = 1usize << exponent;
        let per = (4096 / sector) as u16;
        for (nsid, lba, blocks) in [
            (2, 0, 1),
            (1, 0, 0),
            (1, 127, 2),
            (1, 128, 1),
            (1, u64::MAX, 1),
            (1, 0, per + 1),
        ] {
            assert!(c.submit(Read { nsid, lba, blocks }, 50).is_err());
            assert_eq!(u32_at(&m.bar.0, 0x1008), 0);
        }
        for number in 0..34 {
            let blocks = if number % 2 == 0 { per } else { 1 };
            let lba = if number % 2 == 0 { 0 } else { 127 };
            let cmd = Read {
                nsid: 1,
                lba,
                blocks,
            };
            c.submit(cmd, 60).unwrap();
            assert_eq!(c.submit(cmd, 61), Err(Error::Busy));
            assert_eq!(m.opcode(1), 2);
            let sq_at = 64 * ((number as usize) % 16);
            assert_eq!(u32_at(&m.pages[2].0, sq_at + 40), lba as u32);
            assert_eq!(u32_at(&m.pages[2].0, sq_at + 48), u32::from(blocks - 1));
            assert_eq!(u32_at(&m.pages[2].0, sq_at + 24), 0x14000);
            let len = usize::from(blocks) * sector;
            let input: Vec<u8> = (0..len)
                .map(|i| (i as u8).wrapping_mul(19) ^ number)
                .collect();
            m.pages[4].0[..len].copy_from_slice(&input);
            assert!(c.read_data(0, &mut [0]).is_err());
            m.complete(1, false, 0);
            assert_eq!(c.poll(62).unwrap().unwrap().bytes, len);
            let mut read = vec![0; len];
            c.read_data(0, &mut read).unwrap();
            assert_eq!(read, input);
            assert!(c.read_data(len, &mut [0]).is_err());
            assert!(c.read_data(usize::MAX, &mut [0]).is_err());
        }
    }
}

#[test]
fn release_requires_deleted_queues_shutdown_and_reset_with_matching_epoch() {
    let mut m = Model::new();
    let mut c = m.activate(9);
    assert!(c.shutdown(40).is_err());
    for command in [DeleteSq, DeleteCq] {
        c.submit(command, 40).unwrap();
        m.complete(0, false, 0);
        c.poll(41).unwrap();
    }
    c.shutdown(42).unwrap();
    assert!(!c.stopped(43).unwrap());
    assert!(c.drained().is_none());
    put32(&mut m.bar.0, 0x1c, 1 | 8);
    assert!(!c.stopped(44).unwrap());
    assert_eq!(u32_at(&m.bar.0, 0x14), 0);
    assert!(c.drained().is_none());
    put32(&mut m.bar.0, 0x1c, 0);
    assert!(c.stopped(45).unwrap());
    let proof = c.drained().unwrap();
    let mut memory = m.memory();
    assert!(proof.confirmed(&memory));
    memory.epoch += 1;
    assert!(!proof.confirmed(&memory));
    put32(&mut m.bar.0, 0x1c, 1);
    assert!(!proof.confirmed(&m.memory()));
}

#[test]
fn successful_cqe_with_iommu_fault_cannot_publish_even_zero_data() {
    let mut m = Model::new();
    let mut c = m.activate(9);
    assert!(c.idle_before_read(&m.memory()));
    c.submit(
        Read {
            nsid: 1,
            lba: 0,
            blocks: 1,
        },
        60,
    )
    .unwrap();
    assert!(!c.idle_before_read(&m.memory()));
    m.complete(1, false, 0);
    assert!(matches!(c.poll_checked(61, || false), Err(Error::DmaFault)));
    let mut data = [0xff; 16];
    assert!(c.read_data(0, &mut data).is_err());
    assert_eq!(data, [0xff; 16]);
    assert!(c.drained().is_none());
}

#[test]
fn write_uses_only_immutable_source_and_flush_finishes_separately() {
    for sector in [9, 12] {
        let mut m = Model::new();
        m.writable = true;
        let mut c = m.activate(sector);
        let size = 1usize << sector;
        let blocks = (4096 / size) as u16;
        let cmd = Write {
            nsid: 1,
            lba: 128 - u64::from(blocks),
            blocks,
        };
        let mut payload: Vec<_> = (0..4096).map(|n| (n as u8).wrapping_mul(19)).collect();
        assert!(c.submit(cmd, 49).is_err());
        assert!(c.submit_write(cmd, &payload[..4095], 49).is_err());
        assert!(c.submit_write(Flush { nsid: 1 }, &[], 49).is_err());
        c.submit_write(cmd, &payload, 50).unwrap();
        let cid1 = c.inflight_cid().unwrap();
        assert_eq!(m.opcode(1), 1);
        assert_eq!(u32_at(&m.pages[2].0, 4), 1);
        assert_eq!(u32_at(&m.pages[2].0, 24), 0x15000);
        assert_eq!(u32_at(&m.pages[2].0, 48), u32::from(blocks - 1));
        assert_eq!(&m.pages[5].0[..], &payload);
        payload.fill(0xff);
        assert_eq!(c.submit_write(cmd, &payload, 51), Err(Error::Busy));
        assert_eq!(c.submit(Flush { nsid: 1 }, 51), Err(Error::Busy));
        assert_ne!(m.pages[5].0[0], payload[0]);
        m.complete(1, false, 0);
        assert_eq!(c.poll(52).unwrap().unwrap().cid, cid1);
        assert_eq!(c.inflight_cid(), None);
        assert!(c.read_data(0, &mut [0; 1]).is_err());
        assert!(c.submit(Flush { nsid: 0xffff_ffff }, 53).is_err());
        c.submit(Flush { nsid: 1 }, 53).unwrap();
        let cid2 = c.inflight_cid().unwrap();
        assert_ne!(cid1, cid2);
        assert_eq!(m.opcode(1), 0);
        let flush = &m.pages[2].0[64..128];
        assert_eq!(u32_at(flush, 4), 1);
        assert!(flush[8..].iter().all(|&b| b == 0));
        assert!(c.poll(54).unwrap().is_none());
        m.complete(1, false, 0);
        assert_eq!(c.poll(55).unwrap().unwrap().cid, cid2);
        let cmd = Write {
            nsid: 1,
            lba: 0,
            blocks: 1,
        };
        c.submit_write(cmd, &payload[..size], 56).unwrap();
        assert!(m.pages[5].0[size..].iter().all(|&b| b == 0));
    }
}

#[test]
fn readonly_role_foreign_namespace_bounds_and_source_aliases_are_rejected() {
    let mut m = Model::new();
    let mut c = m.activate(9);
    let valid = Write {
        nsid: 1,
        lba: 1,
        blocks: 1,
    };
    assert!(c.submit_write(valid, &[0; 512], 40).is_err());
    assert!(c.submit(Flush { nsid: 1 }, 40).is_err());
    assert_eq!(u32_at(&m.bar.0, 0x1008), 0);
    let mut m = Model::new();
    m.writable = true;
    for corrupt in 0..4 {
        let mut mem = m.memory();
        match corrupt {
            0 => mem.cpu[5] = mem.cpu[3],
            1 => mem.iova[5] = mem.iova[2],
            2 => mem.iova[5] = 0,
            _ => mem.cpu[5] = 0,
        }
        assert!(matches!(
            unsafe { Controller::prepare(mem) },
            Err(Error::Memory)
        ));
    }
    let mut c = m.activate(9);
    for (nsid, lba, blocks) in [
        (2, 0, 1),
        (1, 127, 2),
        (1, 128, 1),
        (1, u64::MAX, 1),
        (1, 0, 0),
        (1, 0, 9),
    ] {
        assert!(
            c.submit_write(Write { nsid, lba, blocks }, &[0; 512], 45)
                .is_err()
        );
    }
    assert_eq!(u32_at(&m.bar.0, 0x1008), 0);
}

#[test]
fn write_or_flush_dma_fault_timeout_or_bad_cqe_never_confirms() {
    for op in [
        Write {
            nsid: 1,
            lba: 8,
            blocks: 1,
        },
        Flush { nsid: 1 },
    ] {
        for failure in 0..4 {
            let mut m = Model::new();
            m.writable = true;
            let mut c = m.activate(9);
            if matches!(op, Write { .. }) {
                c.submit_write(op, &[0xab; 512], 60).unwrap();
            } else {
                c.submit(op, 60).unwrap();
            }
            let cid = c.inflight_cid().unwrap();
            m.complete(1, failure == 2, if failure == 3 { 0x181 } else { 0 });
            let result = c.poll_checked(if failure == 1 { 5060 } else { 61 }, || failure != 0);
            let expected = [
                Error::DmaFault,
                Error::Timeout,
                Error::Completion,
                Error::Status(0x181),
            ];
            assert_eq!(result.unwrap_err(), expected[failure]);
            assert!(c.poll(5061).is_err());
            assert_eq!(c.inflight_cid(), Some(cid));
            assert!(c.drained().is_none());
            if matches!(op, Write { .. }) {
                assert_eq!(&m.pages[5].0[..512], &[0xab; 512]);
            }
        }
    }
}

#[test]
fn a_writable_lease_cannot_adopt_the_phase_one_disk() {
    let mut m = Model::new();
    m.writable = true;
    let mut c = m.start();
    c.submit(IdentifyController, 30).unwrap();
    m.writable = false;
    m.identify(IdentifyController, 9);
    m.complete(0, false, 0);
    assert!(matches!(c.poll(31), Err(Error::Identity)));
}

#[test]
fn writable_media_reopens_with_only_five_readonly_lease_pages() {
    let mut m = Model::new();
    let mut mem = m.memory();
    mem.phase2_media = true;
    let mut c = unsafe { Controller::prepare(mem).unwrap() };
    c.enable(0).unwrap();
    put32(&mut m.bar.0, 0x1c, 1);
    assert!(c.ready(1).unwrap());
    m.writable = true; // Identify répond avec la série du média de phase 2.
    for command in [
        IdentifyController,
        ListNamespaces,
        IdentifyNamespace,
        NumberQueues,
        CreateCq,
        CreateSq,
    ] {
        c.submit(command, 5).unwrap();
        m.identify(command, 9);
        m.complete(0, false, 0);
        c.poll(6).unwrap().unwrap();
    }
    assert!(
        c.submit_write(
            Write {
                nsid: 1,
                lba: 0,
                blocks: 1
            },
            &[1; 512],
            10
        )
        .is_err()
    );
    assert!(c.submit(Flush { nsid: 1 }, 10).is_err());
    c.submit(
        Read {
            nsid: 1,
            lba: 0,
            blocks: 1,
        },
        10,
    )
    .unwrap();
}
