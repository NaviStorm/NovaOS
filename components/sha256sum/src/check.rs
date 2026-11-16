use crate::{
    Platform, digest,
    manifest::{entry, hex, quote},
    options::{Options, Report},
};
use alloc::{format, vec, vec::Vec};

#[derive(Default)]
struct Totals {
    valid: bool,
    matched: bool,
    malformed: u64,
    unreadable: u64,
    wrong: u64,
    reversed: Option<bool>,
}

impl Totals {
    fn line<P: Platform>(
        &mut self,
        io: &mut P,
        opt: &Options,
        manifest: &[u8],
        line: &[u8],
        n: u64,
        oversized: bool,
    ) -> Result<(), P::Error> {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.starts_with(b"#") || line.is_empty() && !oversized {
            return Ok(());
        }
        let parsed = entry(line, &mut self.reversed)
            .filter(|(_, p)| !(p == b"-" && manifest == b"-") && !oversized);
        let Some((expected, name)) = parsed else {
            self.malformed += 1;
            if opt.report == Report::Warn {
                io.message(
                    &[
                        &label(manifest),
                        format!(": {n}: improperly formatted SHA256 checksum line").as_bytes(),
                    ]
                    .concat(),
                )?;
            }
            return Ok(());
        };
        self.valid = true;
        let status = match digest(io, &name) {
            Ok(hash) if hex(&hash).eq_ignore_ascii_case(expected) => {
                self.matched = true;
                if opt.report == Report::Quiet {
                    return Ok(());
                }
                b": OK\n".as_slice()
            }
            Ok(_) => {
                self.wrong += 1;
                b": FAILED\n"
            }
            Err(e) if opt.ignore && P::missing(&e) => return Ok(()),
            Err(e) => {
                self.unreadable += 1;
                io.file_error(&name, &e)?;
                b": FAILED open or read\n"
            }
        };
        if opt.report != Report::Status {
            io.write(false, &[quote(&name).as_slice(), status].concat())?;
        }
        Ok(())
    }
    fn finish<P: Platform>(
        &self,
        io: &mut P,
        opt: &Options,
        path: &[u8],
    ) -> Result<bool, P::Error> {
        if !self.valid {
            io.message(
                &[
                    label(path).as_slice(),
                    b": no properly formatted checksum lines found",
                ]
                .concat(),
            )?;
        } else if opt.report != Report::Status {
            for (count, single, plural) in [
                (
                    self.malformed,
                    "line is improperly formatted",
                    "lines are improperly formatted",
                ),
                (
                    self.unreadable,
                    "listed file could not be read",
                    "listed files could not be read",
                ),
                (
                    self.wrong,
                    "computed checksum did NOT match",
                    "computed checksums did NOT match",
                ),
            ] {
                if count > 0 {
                    io.message(
                        format!(
                            "WARNING: {count} {}",
                            if count == 1 { single } else { plural }
                        )
                        .as_bytes(),
                    )?;
                }
            }
            if opt.ignore && !self.matched {
                io.message(&[label(path).as_slice(), b": no file was verified"].concat())?;
            }
        }
        Ok(self.matched
            && self.unreadable == 0
            && self.wrong == 0
            && (!opt.strict || self.malformed == 0))
    }
}
fn label(path: &[u8]) -> Vec<u8> {
    quote(if path == b"-" {
        b"standard input"
    } else {
        path
    })
}

pub fn verify<P: Platform>(io: &mut P, path: &[u8], opt: &Options) -> Result<bool, P::Error> {
    let mut reader = match io.open(path) {
        Ok(s) => s,
        Err(e) => {
            io.file_error(path, &e)?;
            return Ok(false);
        }
    };
    let mut buffer = vec![0; P::BUFFER];
    let mut line = Vec::new();
    let mut oversized = false;
    let mut n = 1;
    let mut totals = Totals::default();
    loop {
        let count = match io.read(&mut reader, &mut buffer) {
            Ok(n) => n,
            Err(e) => {
                io.file_error(path, &e)?;
                return Ok(false);
            }
        };
        if count == 0 {
            break;
        }
        for part in buffer[..count].split_inclusive(|&c| c == b'\n') {
            let content = part.strip_suffix(b"\n").unwrap_or(part);
            // Les noms admis sur Darwin et P9 tiennent dans cette fenêtre, même échappés.
            let take = content.len().min(16_384 - line.len());
            line.extend_from_slice(&content[..take]);
            oversized |= take != content.len();
            if part.ends_with(b"\n") {
                totals.line(io, opt, path, &line, n, oversized)?;
                n += 1;
                line.clear();
                oversized = false;
            }
        }
    }
    if !line.is_empty() {
        totals.line(io, opt, path, &line, n, oversized)?;
    }
    totals.finish(io, opt, path)
}
