use super::*;

#[test]
fn exact_features_no_offload_or_bypass() {
    for missing in [MAC, ACCESS_PLATFORM, VERSION_1] {
        assert_eq!(features(!missing), Err(Error::Features));
    }
    assert_eq!(features(u64::MAX), Ok(MAC | ACCESS_PLATFORM | VERSION_1));
}

#[test]
fn frame_bounds_and_padding() {
    let src = [0xb6; MAX_ETH + 1];
    let mut slot = [0xff; SLOT_BYTES];
    for len in [14, 59, 60, 64, 1514] {
        slot.fill(0xff);
        let end = encode_tx(&src[..len], &mut slot).unwrap();
        assert_eq!(end, HEADER + len.max(MIN_WIRE));
        assert_eq!(&slot[HEADER..HEADER + len], &src[..len]);
        assert!(
            slot[..HEADER]
                .iter()
                .chain(&slot[HEADER + len..end])
                .all(|&b| b == 0)
        );
        assert!(slot[end..].iter().all(|&b| b == 0xff));
        slot[10] = 1;
        assert_eq!(decode_rx(&slot[..end]).unwrap(), &slot[HEADER..end]);
    }
    let before = slot;
    assert_eq!(encode_tx(&src[..13], &mut slot), Err(Error::Frame));
    assert_eq!(encode_tx(&src, &mut slot), Err(Error::Frame));
    assert_eq!(encode_tx(&src[..60], &mut slot[..65]), Err(Error::Frame));
    assert_eq!(slot, before);
}

#[test]
fn malformed_rx_delivers_no_slice() {
    let mut rx = [0; SLOT_BYTES];
    rx[10] = 1;
    for size in [0, HEADER - 1, HEADER + 13, HEADER + MAX_ETH + 1] {
        assert_eq!(decode_rx(&rx[..size]), Err(Error::Frame));
    }
    for (byte, val) in [(0, 1), (1, 3), (10, 0), (10, 2), (11, 1)] {
        let mut bad = rx;
        bad[byte] = val;
        assert_eq!(decode_rx(&bad[..90]), Err(Error::Frame));
    }
}

#[test]
fn saturation_returns_all_slots_out_of_order_and_wraps() {
    for dir in [Direction::Transmit, Direction::Receive] {
        let mut queue = Queue::new(dir);
        let mut used = 0u16;
        for _ in 0..6250 {
            for id in 0..SLOTS {
                assert_eq!(queue.reserve(), Ok(id));
            }
            assert_eq!(queue.reserve(), Err(Error::Full));
            used = used.wrapping_add(SLOTS);
            assert_eq!(queue.available_completions(used), Ok(SLOTS));
            for id in (0..SLOTS).rev() {
                queue.complete(u32::from(id), 0).unwrap();
            }
            assert_eq!(queue.pending(), 0);
            assert_eq!(queue.available_completions(used), Ok(0));
        }
    }
}

#[test]
fn invalid_completion_preserves_ownership() {
    for (id, written) in [(0, 1), (2, 0), (65536, 0)] {
        let mut tx = Queue::new(Direction::Transmit);
        tx.reserve().unwrap();
        assert_eq!(tx.complete(id, written), Err(Error::Quarantined));
        assert_eq!(tx.pending(), 1);
        assert_eq!(tx.complete(0, 0), Err(Error::Quarantined));
        assert_eq!(tx.reserve(), Err(Error::Quarantined));
    }
    let mut rx = Queue::new(Direction::Receive);
    rx.reserve().unwrap();
    assert_eq!(
        rx.complete(0, SLOT_BYTES as u32 + 1),
        Err(Error::Quarantined)
    );
    assert_eq!(rx.pending(), 1);
}

#[test]
fn duplicate_and_impossible_used_index_stop_the_queue() {
    let mut tx = Queue::new(Direction::Transmit);
    tx.reserve().unwrap();
    tx.complete(0, 0).unwrap();
    assert_eq!(tx.complete(0, 0), Err(Error::Quarantined));
    let mut rx = Queue::new(Direction::Receive);
    rx.reserve().unwrap();
    assert_eq!(rx.available_completions(2), Err(Error::Quarantined));
    assert_eq!(rx.pending(), 1);
}

#[derive(Default)]
struct Device {
    status: u8,
    accepted: u64,
    queues: Vec<Direction>,
    posted: Vec<u16>,
    deny_features: bool,
    fail_post: bool,
    fail_retire: bool,
    retired: bool,
    kicked: bool,
}

impl init::Control for Device {
    fn reset(&mut self) -> Result<(), init::InitError> {
        self.status = 0;
        Ok(())
    }
    fn status(&mut self) -> Result<u8, init::InitError> {
        Ok(self.status)
    }
    fn set_status(&mut self, value: u8) -> Result<(), init::InitError> {
        assert_eq!(self.status & value, self.status);
        self.status = if self.deny_features {
            value & !init::FEATURES_OK
        } else {
            value
        };
        if self.status & init::DRIVER_OK != 0 {
            assert_eq!(self.posted.len(), usize::from(SLOTS));
        }
        Ok(())
    }
    fn offered(&mut self) -> Result<u64, init::InitError> {
        Ok(u64::MAX)
    }
    fn accept(&mut self, selected: u64) -> Result<(), init::InitError> {
        assert_eq!(self.status, init::ACK | init::DRIVER);
        self.accepted = selected;
        Ok(())
    }
    fn mac(&mut self) -> Result<[u8; 6], init::InitError> {
        Ok([2, 0, 0, 0, 0, 1])
    }
    fn max_queue(&mut self, _: Direction) -> Result<u16, init::InitError> {
        Ok(256)
    }
    fn setup(&mut self, dir: Direction, size: u16) -> Result<(), init::InitError> {
        assert_ne!(self.status & init::FEATURES_OK, 0);
        assert_eq!(size, SLOTS);
        self.queues.push(dir);
        Ok(())
    }
    fn post_rx(&mut self, id: u16) -> Result<(), init::InitError> {
        if self.fail_post && id == 2 {
            return Err(init::InitError::Transport);
        }
        assert_eq!(self.posted.len(), usize::from(id));
        self.posted.push(id);
        Ok(())
    }
    fn kick_rx(&mut self) -> Result<(), init::InitError> {
        assert_ne!(self.status & init::DRIVER_OK, 0);
        self.kicked = true;
        Ok(())
    }
    fn retire(&mut self) -> Result<(), init::InitError> {
        self.retired = true;
        if self.fail_retire {
            Err(init::InitError::Transport)
        } else {
            Ok(())
        }
    }
}

#[test]
fn startup_orders_features_queues_and_receive_buffers_before_ready() {
    let mut dev = Device::default();
    let link = init::initialise(&mut dev).unwrap();
    assert_eq!(dev.accepted, MAC | VERSION_1 | ACCESS_PLATFORM);
    assert_eq!(dev.queues, [Direction::Receive, Direction::Transmit]);
    assert_eq!(link.rx.pending(), SLOTS);
    assert_eq!(link.tx.pending(), 0);
    assert_eq!(link.mac, [2, 0, 0, 0, 0, 1]);
    assert!(dev.kicked && !dev.retired);
}

#[test]
fn rejected_negotiation_and_partial_posting_retire_before_return() {
    for (deny_features, fail_post, fail_retire, expected) in [
        (true, false, false, init::InitError::Status),
        (false, true, false, init::InitError::Transport),
        (false, true, true, init::InitError::Quarantined),
    ] {
        let mut dev = Device {
            deny_features,
            fail_post,
            fail_retire,
            ..Device::default()
        };
        assert_eq!(init::initialise(&mut dev).err(), Some(expected));
        assert!(dev.retired && !dev.kicked);
        if deny_features {
            assert!(dev.queues.is_empty());
        }
    }
}
