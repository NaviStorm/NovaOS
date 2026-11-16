use crate::{Source, check, digest, manifest, options::Options};
use alloc::string::String;
pub trait Platform: Source {
    fn write(&mut self, err: bool, bytes: &[u8]) -> Result<(), Self::Error>;
    fn missing(error: &Self::Error) -> bool;
    fn reason(error: &Self::Error) -> String;
    fn message(&mut self, bytes: &[u8]) -> Result<(), Self::Error> {
        self.write(true, &[b"sha256sum: ", bytes, b"\n"].concat())
    }
    fn file_error(&mut self, path: &[u8], error: &Self::Error) -> Result<(), Self::Error> {
        self.message(
            &[
                manifest::quote(path).as_slice(),
                b": ",
                Self::reason(error).as_bytes(),
            ]
            .concat(),
        )
    }
}
pub fn run<P: Platform>(io: &mut P, args: &[&[u8]], fr: bool) -> Result<bool, P::Error> {
    let options = match Options::parse(args) {
        Ok(opt) => opt,
        Err(e) => {
            io.message(e.as_bytes())?;
            io.write(true, b"Try 'sha256sum --help' for more information.\n")?;
            return Ok(false);
        }
    };
    if let Some(help) = options.doc {
        let text = if !help {
            concat!("sha256sum (NovaOS) ", env!("CARGO_PKG_VERSION"), "\n")
        } else if fr {
            include_str!("../help.fr")
        } else {
            include_str!("../help.en")
        };
        io.write(false, text.as_bytes())?;
        return Ok(true);
    }
    let mut success = true;
    for &path in &options.paths {
        if options.check {
            success &= check::verify(io, path, &options)?;
        } else {
            match digest(io, path) {
                Ok(hash) => io.write(false, &manifest::record(&hash, path, &options))?,
                Err(e) => {
                    io.file_error(path, &e)?;
                    success = false;
                }
            }
        }
    }
    Ok(success)
}
