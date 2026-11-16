//! Vecteurs de l’oracle GNU embarqués uniquement dans l’ELF témoin SDK.
use alloc::vec::Vec;
use nova_sort::{
    args::Options,
    engine::{Order, merge},
};
pub fn verify() -> bool {
    for &(args, data, expected) in CASES {
        let Ok(o) = Options::parse(args) else {
            return false;
        };
        let order = Order {
            options: &o,
            collate: None,
            random_key: None,
            general: super::native::general,
        };
        let d = o.delim();
        let mut records: Vec<_> = data
            .split_inclusive(|&b| b == d)
            .map(|s| s.strip_suffix(&[d]).unwrap_or(s).to_vec())
            .collect();
        // Force a stable multi-run merge, including duplicate keys on both sides.
        for run in records.chunks_mut(2) {
            order.sort(run)
        }
        let mut offsets = alloc::vec![0;records.len().div_ceil(2)];
        let mut observed = Vec::new();
        let mut last: Option<Vec<u8>> = None;
        let result = merge(
            offsets.len(),
            &order,
            |i, row| {
                let end = (i * 2 + 2).min(records.len());
                let n = i * 2 + offsets[i];
                if n == end {
                    return Ok::<_, ()>(false);
                }
                row.clone_from(&records[n]);
                offsets[i] += 1;
                Ok(true)
            },
            |row| {
                if !o.unique || !last.as_ref().is_some_and(|p| order.compare(p, row).is_eq()) {
                    observed.extend_from_slice(row);
                    observed.push(d);
                    last = Some(row.to_vec())
                }
                Ok(())
            },
        );
        if result.is_err() || observed != expected {
            return false;
        }
    }
    true
}
include!("golden.rs");
