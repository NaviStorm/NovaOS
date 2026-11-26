//! Exécuter un processus neuf par cellule et série.
use ethernet_v1::{
    Binding,
    transport::{Channel, WINDOW},
    wire::{self, Header},
};
use std::{hint::black_box, time::Instant};

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct Usage {
    cpu_ns: u64,
    cycles: u64,
    rss: u64,
    peak: u64,
    switches: u64,
    wake_irq: u64,
}
unsafe extern "C" {
    fn ethernet_usage(out: *mut Usage) -> i32;
}
fn usage() -> Usage {
    let mut u = Usage::default();
    assert_eq!(unsafe { ethernet_usage(&mut u) }, 0);
    u
}
fn binding(owner: u64) -> Binding {
    Binding {
        boot: [1; 16],
        port: 91,
        owner,
        incarnation: 1,
        channel: owner,
        epoch: 1,
    }
}
fn cell<const N: usize>(b: usize, len: usize, mode: &str, series: u64) {
    const WARM: usize = 128;
    const RUNS: usize = 4096;
    let src = binding(2);
    let dst = binding(3);
    let mut bytes = vec![0u8; b * len];
    let mut result = vec![0u8; b * len];
    for (i, frame) in bytes.chunks_exact_mut(len).enumerate() {
        for (j, x) in frame.iter_mut().enumerate() {
            *x = (i * 19 + j * 29) as u8;
        }
        frame[..14].copy_from_slice(&[2, 0, 0, 0, 0, 2, 2, 0, 0, 0, 0, 1, 0x88, 0xb5]);
    }
    let mut samples = Vec::with_capacity(RUNS);
    let mut retire = Vec::with_capacity(129);
    let idle = usage();
    let mut ch = Channel::<N>::new(src, dst).unwrap();
    let mut body = [0u8; 96];
    let mut receipt = [0u8; 96];
    let mut data = [0u8; 128];
    let mut reply = [0u8; 128];
    let mut calls = 0;
    let mut copies = 0;
    let mut volume = 0;
    let mut start = idle;
    for i in 0..WARM + RUNS {
        let seq = i as u64 + 1;
        if i == WARM {
            start = usage();
        }
        for (index, frame) in bytes.chunks_exact_mut(len).enumerate() {
            frame[14..22].copy_from_slice(&(seq + index as u64).to_le_bytes());
            body[index * 12..index * 12 + 12].copy_from_slice(&wire::descriptor(
                index as u16,
                len as u16,
                seq as u32,
            ));
            receipt[index * 12..index * 12 + 2].copy_from_slice(&(index as u16).to_le_bytes());
            receipt[index * 12 + 4..index * 12 + 12].copy_from_slice(&seq.to_le_bytes());
        }
        let h = Header {
            op: wire::TX,
            reply: false,
            count: b as u16,
            epoch: src.epoch,
            request: seq,
        };
        let t = Instant::now();
        let n = h.encode(&mut data, &body[..b * 12]).unwrap();
        ch.begin(src, seq).unwrap();
        let c = ch
            .transfer(
                src,
                dst,
                black_box(&data[..n]),
                black_box(&bytes),
                &mut result,
            )
            .unwrap();
        assert_eq!(black_box(&result), &bytes);
        let ack = Header { reply: true, ..h };
        let n = ack.encode(&mut reply, &receipt[..b * 12]).unwrap();
        let (parsed, payload) = Header::decode(black_box(&reply[..n])).unwrap();
        assert_eq!(parsed, ack);
        assert_eq!(payload, &receipt[..b * 12]);
        let elapsed = t.elapsed().as_nanos() as u64;
        if i >= WARM {
            samples.push(elapsed);
            calls += c.calls;
            copies += c.copies;
            volume += c.bytes;
        }
    }
    let end = usage();
    for _ in 0..129 {
        let t = Instant::now();
        ch.revoke();
        retire.push(t.elapsed().as_nanos() as u64);
        ch = Channel::<N>::new(src, dst).unwrap();
        // Laisser un vrai lot et des octets sensibles à nettoyer entre mesures.
        ch.begin(src, 1).unwrap();
        let h = Header {
            op: wire::TX,
            reply: false,
            count: b as u16,
            epoch: 1,
            request: 1,
        };
        let n = h.encode(&mut data, &body[..b * 12]).unwrap();
        ch.transfer(src, dst, &data[..n], &bytes, &mut result)
            .unwrap();
    }
    println!(
        "CELL mode={mode} b={b} len={len} series={series} calls={calls} copies={copies} bytes={volume} cpu_ns={} cycles={} rss_base={} rss={} peak={} switches={} wakes_irq={} model_bytes={}",
        end.cpu_ns - start.cpu_ns,
        end.cycles - start.cycles,
        idle.rss,
        end.rss,
        end.peak,
        end.switches - start.switches,
        end.wake_irq - start.wake_irq,
        std::mem::size_of::<Channel<N>>()
    );
    println!("NS {:?}", samples);
    println!("REVOKE {:?}", retire);
}
fn main() {
    let a: Vec<_> = std::env::args().skip(1).collect();
    let mode = a[0].as_str();
    let b = a[1].parse().unwrap();
    let len = a[2].parse().unwrap();
    let s = a[3].parse().unwrap();
    assert!([1, 2, 4, 8].contains(&b) && [60, 1514].contains(&len));
    match mode {
        "data" => cell::<128>(b, len, mode, s),
        "boite" => cell::<4096>(b, len, mode, s),
        "fenetre" => cell::<WINDOW>(b, len, mode, s),
        _ => panic!("transport"),
    }
}
