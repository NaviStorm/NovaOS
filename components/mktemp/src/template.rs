use super::options::Options;
use alloc::vec::Vec;
use core::ops::Range;

pub struct Template {
    pub name: Vec<u8>,
    pub letters: Range<usize>,
}

pub fn prepare(opt: &Options, env: &[u8], fallback: &[u8]) -> Result<Template, Vec<u8>> {
    let raw = opt.template.unwrap_or(b"tmp.XXXXXXXXXX");
    let mut end = raw.len();
    let suffix = match opt.suffix {
        Some(value) if raw.ends_with(b"X") => value,
        Some(_) => {
            return Err([
                b"with --suffix, template '".as_slice(),
                &super::quote::escaped(raw),
                b"' must end in X",
            ]
            .concat());
        }
        None => {
            end = raw
                .iter()
                .rposition(|&b| b == b'X')
                .map_or(raw.len(), |i| i + 1);
            &raw[end..]
        }
    };
    if suffix.contains(&b'/') {
        return Err([
            b"invalid suffix '".as_slice(),
            &super::quote::escaped(suffix),
            b"', contains directory separator",
        ]
        .concat());
    }
    let count = raw[..end].iter().rev().take_while(|&&c| c == b'X').count();
    if count < 3 {
        return Err([
            b"too few X's in template '".as_slice(),
            &super::quote::escaped(raw),
            b"'",
        ]
        .concat());
    }
    let mut name = Vec::new();
    if opt.template.is_none() || opt.parent.is_some() || opt.legacy {
        let specified = opt.parent.unwrap_or_default();
        let choices = if opt.legacy {
            [env, specified]
        } else {
            [specified, env]
        };
        let parent = choices
            .into_iter()
            .find(|v| !v.is_empty())
            .unwrap_or(fallback);
        let reason = if opt.legacy && raw.contains(&b'/') {
            Some(b"', contains directory separator".as_slice())
        } else if raw.starts_with(b"/") {
            Some(b"'; with --tmpdir, it may not be absolute".as_slice())
        } else {
            None
        };
        if let Some(why) = reason {
            return Err([
                b"invalid template, '".as_slice(),
                &super::quote::escaped(&[&raw[..end], suffix].concat()),
                why,
            ]
            .concat());
        }
        let keep = parent
            .iter()
            .rposition(|&c| c != b'/')
            .map_or(parent.len(), |i| i + 1);
        name.extend_from_slice(&parent[..keep]);
        if !name.ends_with(b"/") {
            name.push(b'/');
        }
    }
    let stop = name.len() + end;
    name.extend_from_slice(&raw[..end]);
    name.extend_from_slice(suffix);
    Ok(Template {
        name,
        letters: stop - count..stop,
    })
}

// Une réservation occupée est le seul résultat qui autorise une nouvelle tentative.
pub fn reserve<E>(
    template: &Template,
    mut fill: impl FnMut(&mut [u8]),
    mut claim: impl FnMut(&[u8]) -> Result<bool, E>,
) -> Result<Option<Vec<u8>>, E> {
    let mut candidate = template.name.clone();
    for _ in 0..238328 {
        fill(&mut candidate[template.letters.clone()]);
        if claim(&candidate)? {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}
