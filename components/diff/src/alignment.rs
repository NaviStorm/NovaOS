//! Une ligne conservée est une ancre commune. Les trous entre ancres décrivent
//! les ajouts et suppressions indépendamment de leur format de présentation.
use super::*;
use engine::Change;

fn gaps(len: usize, anchors: &[usize]) -> Vec<usize> {
    let mut prev = 0;
    let mut result = vec![];
    for &node in anchors {
        result.push(node - prev);
        prev = node + 1
    }
    result.push(len - prev);
    result
}
fn slide(seq: &[usize], anchors: &mut [usize], gaps: &mut [usize], opposite: &[usize]) {
    let mut i = 0;
    while i < gaps.len() {
        if gaps[i] == 0 {
            i += 1;
            continue;
        }
        // Chercher toutes les positions équivalentes du trou. Un regroupement
        // libère parfois d'autres positions, d'où la reprise après une fusion.
        loop {
            let mut amount = gaps[i];
            while i > 0 {
                let pivot = anchors[i - 1];
                if seq[pivot] != seq[pivot + amount] {
                    break;
                }
                anchors[i - 1] += amount;
                gaps[i] = 0;
                i -= 1;
                gaps[i] += amount;
                amount = gaps[i];
            }
            let mut aligned = (opposite[i] != 0).then_some(i);
            let mut merged = false;
            while i < anchors.len() {
                let pivot = anchors[i];
                if seq[pivot - amount] != seq[pivot] {
                    break;
                }
                anchors[i] -= amount;
                gaps[i] = 0;
                i += 1;
                merged |= gaps[i] != 0;
                gaps[i] += amount;
                amount = gaps[i];
                if opposite[i] != 0 {
                    aligned = Some(i)
                }
            }
            if merged {
                continue;
            }
            if let Some(target) = aligned {
                while i > target {
                    anchors[i - 1] += amount;
                    gaps[i] = 0;
                    i -= 1;
                    gaps[i] = amount
                }
            }
            break;
        }
        i += 1;
    }
}
pub fn changes(a: &[usize], b: &[usize], count: usize, o: &Options, base: usize) -> Vec<Change> {
    let mut freq = [vec![0usize; count], vec![0usize; count]];
    for (side, seq) in [a, b].iter().enumerate() {
        for &k in *seq {
            freq[side][k] += 1
        }
    }
    let mut anchors = [vec![], vec![]];
    let index = [a, b].map(|seq| (0..seq.len()).collect::<Vec<_>>());
    let [mut ax, mut bx] = index;
    if !o.has("minimal") {
        ax.retain(|&i| freq[1][a[i]] != 0);
        bx.retain(|&i| freq[0][b[i]] != 0);
    }
    let compact = [
        ax.iter().map(|&i| a[i]).collect::<Vec<_>>(),
        bx.iter().map(|&i| b[i]).collect(),
    ];
    let mut raw = vec![];
    // Des ancres uniques dans le même ordre séparent les grandes plages.
    // En mode minimal, la recherche exacte porte toujours sur toute la plage.
    let mut cuts = vec![];
    if o.has("speed-large-files")
        && !o.has("minimal")
        && compact[0].len().min(compact[1].len()) > 4096
    {
        let mut at = vec![0; count];
        for (j, &key) in compact[1].iter().enumerate() {
            at[key] = j
        }
        let mut last = None;
        for (i, &key) in compact[0].iter().enumerate() {
            if freq[0][key] == 1 && freq[1][key] == 1 {
                if last.is_some_and(|j| j >= at[key]) {
                    cuts.clear();
                    break;
                }
                last = Some(at[key]);
                if i % 256 == 0 {
                    cuts.push((i, at[key]))
                }
            }
        }
    }
    let (mut lo, mut ro) = (0, 0);
    for (i, j) in cuts {
        engine::differences(&compact[0][lo..i], &compact[1][ro..j], lo, ro, &mut raw);
        lo = i + 1;
        ro = j + 1;
    }
    engine::differences(&compact[0][lo..], &compact[1][ro..], lo, ro, &mut raw);
    let (mut x, mut y) = (0, 0);
    for h in &raw {
        anchors[0].extend_from_slice(&ax[x..h.a.start]);
        anchors[1].extend_from_slice(&bx[y..h.b.start]);
        x = h.a.end;
        y = h.b.end;
    }
    anchors[0].extend_from_slice(&ax[x..]);
    anchors[1].extend_from_slice(&bx[y..]);
    let [mut left, mut right] = anchors;
    let mut ga = gaps(a.len(), &left);
    let mut gb = gaps(b.len(), &right);
    slide(a, &mut left, &mut ga, &gb);
    slide(b, &mut right, &mut gb, &ga);
    let mut out = vec![];
    let (mut x, mut y) = (base, base);
    for (nx, ny) in ga.into_iter().zip(gb) {
        if nx != 0 || ny != 0 {
            out.push(Change {
                a: x..x + nx,
                b: y..y + ny,
            })
        }
        x += nx + 1;
        y += ny + 1
    }
    out
}
