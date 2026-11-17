use super::*;
use alloc::collections::BTreeMap;
use core::ops::Range;
use posix_regex::PosixRegex;

#[derive(Clone, Debug)]
pub struct Change {
    pub a: Range<usize>,
    pub b: Range<usize>,
}
pub fn body(s: &[u8]) -> &[u8] {
    s.strip_suffix(b"\n").unwrap_or(s)
}
pub fn matches(rx: &[PosixRegex<'_>], line: &[u8]) -> bool {
    rx.iter()
        .any(|re| !re.matches(body(line), Some(1)).is_empty())
}
fn normalized(line: &[u8], o: &Options) -> Vec<u8> {
    let folded;
    let line = if o.utf8
        && o.has("ignore-case")
        && let Ok(s) = core::str::from_utf8(line)
    {
        folded = s.to_lowercase();
        folded.as_bytes()
    } else {
        line
    };
    let mut result = Vec::with_capacity(line.len());
    let mut space = false;
    let mut col = 0;
    for (i, &c) in body(line).iter().enumerate() {
        if c.is_ascii_whitespace() && (o.has("ignore-all-space") || o.has("ignore-space-change")) {
            space = true;
            continue;
        }
        if space && o.has("ignore-space-change") {
            result.push(b' ')
        }
        space = false;
        if c == b'\t' && o.has("ignore-tab-expansion") {
            let n = o.tab - col % o.tab;
            result.resize(result.len() + n, b' ');
            col += n;
        } else {
            result.push(if o.has("ignore-case") {
                c.to_ascii_lowercase()
            } else {
                c
            });
            col = if c == b'\r' {
                0
            } else if c == 8 {
                col.saturating_sub(1)
            } else if o.utf8 && c >= 128 {
                let w = if c < 192 {
                    0
                } else {
                    let n = if c < 224 {
                        2
                    } else if c < 240 {
                        3
                    } else {
                        4
                    };
                    line.get(i..i + n)
                        .and_then(|b| core::str::from_utf8(b).ok())
                        .and_then(|s| s.chars().next())
                        .and_then(unicode_width::UnicodeWidthChar::width)
                        .unwrap_or(1)
                };
                col + w
            } else {
                col + 1
            }
        }
    }
    if o.has("ignore-trailing-space") {
        while result.last().is_some_and(u8::is_ascii_whitespace) {
            result.pop();
        }
    }
    if line.ends_with(b"\n")
        && !o.has("ignore-all-space")
        && !o.has("ignore-space-change")
        && !o.has("ignore-trailing-space")
    {
        result.push(b'\n')
    }
    result
}
// Recherche bidirectionnelle de Myers. Les deux fronts sont libérés avant la subdivision.
fn middle(a: &[usize], b: &[usize], budget: usize) -> Option<(usize, usize)> {
    let (n, m) = (a.len() as isize, b.len() as isize);
    let delta = n - m;
    let max = (n + m + 1) / 2;
    let offset = max + 1;
    let size = (2 * max + 3) as usize;
    let mut fronts = [vec![-1isize; size], vec![-1isize; size]];
    for f in &mut fronts {
        f[(offset + 1) as usize] = 0;
    }
    for d in 0..=max {
        for rev in 0..2 {
            for diag in (-d..=d).step_by(2) {
                let k = if rev == 0 { -diag } else { diag };
                let i = (offset + k) as usize;
                let f = &fronts[rev];
                let mut x = if k == -d || k != d && f[i - 1] < f[i + 1] {
                    f[i + 1]
                } else {
                    f[i - 1] + 1
                };
                let mut y = x - k;
                if x < 0 || y < 0 || x > n || y > m {
                    continue;
                }
                while x < n && y < m {
                    let (ix, iy) = if rev == 0 {
                        (x, y)
                    } else {
                        (n - 1 - x, m - 1 - y)
                    };
                    if a[ix as usize] != b[iy as usize] {
                        break;
                    }
                    x += 1;
                    y += 1;
                }
                fronts[rev][i] = x;
                let other = delta - k;
                if (delta & 1 == 1) == (rev == 0) && other.abs() <= d - (rev == 0) as isize {
                    let far = fronts[1 - rev][(offset + other) as usize];
                    if far >= 0 && x + far >= n {
                        return Some(if rev == 0 {
                            (x as usize, y as usize)
                        } else {
                            ((n - x) as usize, (m - y) as usize)
                        });
                    }
                }
            }
        }
        if d as usize == budget {
            // Une fois le budget de recherche atteint, couper sur le front qui
            // a couvert le plus de lignes. Chaque front emploie ses coordonnées locales.
            let furthest = |rev: usize| {
                let mut best = (0, 0);
                for t in (-d..=d).step_by(2) {
                    let k = if rev == 0 { -t } else { t };
                    let x = fronts[rev][(offset + k) as usize].min(n).min(m + k);
                    let y = x - k;
                    if x >= 0 && y >= 0 && x + y > best.0 + best.1 {
                        best = (x, y);
                    }
                }
                best
            };
            let left = furthest(0);
            let right = furthest(1);
            let cut = if left.0 + left.1 > right.0 + right.1 {
                left
            } else {
                (n - right.0, m - right.1)
            };
            return Some((cut.0 as usize, cut.1 as usize));
        }
    }
    None
}
pub(super) fn differences(
    mut a: &[usize],
    mut b: &[usize],
    mut i: usize,
    mut j: usize,
    out: &mut Vec<Change>,
    budget: usize,
) {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    i += pre;
    j += pre;
    a = &a[pre..];
    b = &b[pre..];
    let tail = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    a = &a[..a.len() - tail];
    b = &b[..b.len() - tail];
    if a.is_empty() && b.is_empty() {
        return;
    }
    if !a.is_empty()
        && !b.is_empty()
        && let Some((x, y)) = middle(a, b, budget)
        && (x, y) != (0, 0)
        && (x, y) != (a.len(), b.len())
    {
        differences(&a[..x], &b[..y], i, j, out, budget);
        differences(&a[x..], &b[y..], i + x, j + y, out, budget);
        return;
    }
    if let Some(last) = out.last_mut()
        && last.a.end == i
        && last.b.end == j
    {
        last.a.end += a.len();
        last.b.end += b.len();
        return;
    }
    out.push(Change {
        a: i..i + a.len(),
        b: j..j + b.len(),
    });
}
// Retirer uniquement des lignes entières et identiques. Le format normal
// utilise le nombre de lignes du préfixe pour numéroter la fenêtre restante.
fn normal_window<'a>(a: &'a [u8], b: &'a [u8]) -> (&'a [u8], &'a [u8], usize) {
    const BLOCK: usize = 512;
    let blocks = a
        .chunks_exact(BLOCK)
        .zip(b.chunks_exact(BLOCK))
        .take_while(|(x, y)| x == y)
        .count()
        * BLOCK;
    let common = blocks
        + a[blocks..]
            .iter()
            .zip(&b[blocks..])
            .take_while(|(x, y)| x == y)
            .count();
    let prefix = a[..common]
        .iter()
        .rposition(|&c| c == b'\n')
        .map_or(0, |i| i + 1);
    let offset = a[..prefix].iter().filter(|&&c| c == b'\n').count();
    let (a, b) = (&a[prefix..], &b[prefix..]);
    let blocks = a
        .rchunks_exact(BLOCK)
        .zip(b.rchunks_exact(BLOCK))
        .take_while(|(x, y)| x == y)
        .count()
        * BLOCK;
    let tail = blocks
        + a[..a.len() - blocks]
            .iter()
            .rev()
            .zip(b[..b.len() - blocks].iter().rev())
            .take_while(|(x, y)| x == y)
            .count();
    let (mut x, mut y) = (a.len() - tail, b.len() - tail);
    if (x > 0 && a[x - 1] != b'\n') || (y > 0 && b[y - 1] != b'\n') {
        let skip = a[x..]
            .iter()
            .position(|&c| c == b'\n')
            .map_or(tail, |i| i + 1);
        x += skip;
        y += skip;
    }
    (&a[..x], &b[..y], offset)
}

// Des substitutions isolées dont les textes n'existent pas sur l'autre côté
// imposent un alignement unique. Les lignes communes peuvent être répétées.
// En cas de doute sur un seul texte, reprendre le calcul général.
fn isolated_changes(rows: &[Vec<&[u8]>; 2]) -> Option<Vec<Change>> {
    let n = rows[0].len();
    if n < 128 || rows[1].len() != n {
        return None;
    }
    let mut changes = Vec::new();
    let mut words = Vec::new();
    for (i, (&left, &right)) in rows[0].iter().zip(&rows[1]).enumerate() {
        if left == right {
            continue;
        }
        if words.len() > n / 8 || changes.last().is_some_and(|h: &Change| h.a.end == i) {
            return None;
        }
        changes.push(Change {
            a: i..i + 1,
            b: i..i + 1,
        });
        words.extend([(left, false), (right, true)]);
    }
    words.sort_unstable();
    if words
        .windows(2)
        .any(|v| v[0].0 == v[1].0 && v[0].1 != v[1].1)
    {
        return None;
    }
    let mut gap = 0;
    let mut start = 0;
    for stop in changes.iter().map(|h| h.a.start).chain(core::iter::once(n)) {
        for &line in &rows[0][start..stop] {
            // Réutiliser l'intervalle lexical du dernier témoin. Les entrées
            // désordonnées conservent une recherche bornée, même avec doublons.
            if gap > 0 && line <= words[gap - 1].0 || gap < words.len() && line >= words[gap].0 {
                match words.binary_search_by(|&(entry, _)| entry.cmp(line)) {
                    Ok(_) => return None,
                    Err(index) => gap = index,
                }
            }
        }
        start = stop + 1;
    }
    Some(changes)
}
pub struct Text<'a> {
    pub rows: [Vec<&'a [u8]>; 2],
    pub changes: Vec<Change>,
    pub first_line: usize,
}
impl<'a> Text<'a> {
    pub fn new(a: &'a [u8], b: &'a [u8], o: &Options) -> Self {
        let (a, b, first_line) = if o.style == "normal"
            && o.val("horizon-lines").is_none()
            && a.len().max(b.len()) >= 8192
        {
            normal_window(a, b)
        } else {
            (a, b, 0)
        };
        let rows = [a, b].map(|data| data.split_inclusive(|c| *c == b'\n').collect::<Vec<_>>());
        let norm = [
            "ignore-all-space",
            "ignore-space-change",
            "ignore-trailing-space",
            "ignore-tab-expansion",
            "ignore-case",
        ]
        .iter()
        .any(|k| o.has(k));
        if !norm
            && o.style == "normal"
            && !o.has("minimal")
            && !o.has("speed-large-files")
            && let Some(changes) = isolated_changes(&rows)
        {
            return Self {
                rows,
                changes,
                first_line,
            };
        }
        let mut ids = BTreeMap::new();
        let mut intern = |lines: &[&'a [u8]]| {
            lines
                .iter()
                .map(|line| {
                    let key = if norm {
                        alloc::borrow::Cow::Owned(normalized(line, o))
                    } else {
                        alloc::borrow::Cow::Borrowed(*line)
                    };
                    let next = ids.len();
                    *ids.entry(key).or_insert(next)
                })
                .collect::<Vec<_>>()
        };
        // Conserver une marge de contexte rend les lignes répétées disponibles
        // pour le placement des blocs, même près du préfixe ou du suffixe commun.
        let horizon = o
            .val("horizon-lines")
            .and_then(|v| txt(v).parse::<usize>().ok())
            .unwrap_or(0)
            .max(if matches!(o.style, "context" | "unified") {
                o.context
            } else {
                0
            });
        let prefix = rows[0]
            .iter()
            .zip(&rows[1])
            .take_while(|(x, y)| x == y)
            .count()
            .saturating_sub(horizon);
        let tail = rows[0][prefix..]
            .iter()
            .rev()
            .zip(rows[1][prefix..].iter().rev())
            .take_while(|(x, y)| x == y)
            .count()
            .saturating_sub(horizon);
        let x = intern(&rows[0][prefix..rows[0].len() - tail]);
        let y = intern(&rows[1][prefix..rows[1].len() - tail]);
        let changes = alignment::changes(&x, &y, ids.len(), o, prefix);
        Self {
            rows,
            changes,
            first_line,
        }
    }
    pub fn ignored(&self, h: &Change, o: &Options) -> bool {
        self.rows[0][h.a.clone()]
            .iter()
            .chain(&self.rows[1][h.b.clone()])
            .all(|s| {
                let blank = o.has("ignore-blank-lines")
                    && (body(s).is_empty()
                        || (o.has("ignore-all-space")
                            || o.has("ignore-space-change")
                            || o.has("ignore-trailing-space"))
                            && body(s).iter().all(u8::is_ascii_whitespace));
                blank || matches(&o.ignore, s)
            })
    }
}
