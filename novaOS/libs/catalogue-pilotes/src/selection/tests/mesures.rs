use super::*;
use std::{hint::black_box, time::Instant};
#[path = "../../../examples/support/banc.rs"]
mod banc;
#[path = "../../../examples/support/memory.rs"]
mod memory;
#[global_allocator]
static ALLOCATOR: memory::Counter = memory::Counter;
fn latence<T>(n: usize, mut f: impl FnMut() -> T) -> (u128, u128) {
    let mut times = Vec::with_capacity(n);
    for _ in 0..n {
        let start = Instant::now();
        black_box(f());
        times.push(start.elapsed().as_nanos());
    }
    times.sort_unstable();
    (times[n / 2], times[n * 99 / 100])
}
#[test]
#[ignore = "mesure isolée release monothread"]
fn catalogue() {
    let p = banc::policy();
    let isa = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    let t = banc::target(isa);
    let raw = banc::RAW.as_bytes();
    let objs = banc::objects();
    let (m, peak, held) = memory::sample(|| crate::schema::decode::<Manifest>(raw).unwrap());
    std::println!("parseur brut={} pic={} retenu={}", raw.len(), peak, held);
    let padded = alloc::format!("{}{}", banc::RAW, " ".repeat(MAX_MANIFEST - raw.len()));
    let (_, max_peak, _) =
        memory::sample(|| crate::schema::decode::<Manifest>(padded.as_bytes()).unwrap());
    std::println!("parseur blanc 16K pic={max_peak}");
    let (p50, p99) = latence(1001, || resolve(&m.dependency, &p.resources).unwrap());
    std::println!("resolution ns {p50} {p99}");
    let k = banc::signer();
    let (p50, p99) = latence(501, || {
        crate::admission::verify(
            &k.manifest_key,
            b"NovaOS/driver-package/v1\0",
            raw,
            banc::SIG,
        )
        .unwrap()
    });
    std::println!("signature enveloppe ns {p50} {p99}");
    let (p50, p99) = latence(501, || {
        let mut c = Catalog::new(&p);
        c.admit(raw, banc::SIG, &objs).unwrap()
    });
    std::println!("admission complete ns {p50} {p99}");
    for n in [1, 64] {
        let candidates: Vec<_> = (0..n)
            .map(|i| {
                let mut s = banc::RAW.replace(
                    "org.novaos.lab.port",
                    &alloc::format!("org.novaos.lab.port{i:02}"),
                );
                if i + 1 != n {
                    s = s.replace("device=0x1041", "device=0x1042");
                }
                let sig = banc::sign(s.as_bytes(), 1);
                (s, sig)
            })
            .collect();
        let (catalog, peak, live) = memory::sample(|| {
            let mut c = Catalog::new(&p);
            for (s, sig) in &candidates {
                c.admit(s.as_bytes(), sig, &objs).unwrap();
            }
            c
        });
        let (plan, plan_peak, plan_live) = memory::sample(|| catalog.select(&t, None).unwrap());
        let (p50, p99) = latence(1001, || catalog.select(black_box(&t), None).unwrap());
        std::println!(
            "n={n} index={live} admission_pic={peak} selection_pic={plan_peak} plan={plan_live} p50_ns={p50} p99_ns={p99} id={}",
            plan.driver()
        );
    }
}
