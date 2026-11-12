//! Catalogue utilisateur et contrat des arguments pour la sélection explicite d'autorité.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u64)]
pub enum Program {
    Pwd,
    Ls,
    Cat,
    Head,
    Mkdir,
    Rm,
    Probe,
    Cp,
    Mv,
    Touch,
    More,
    Wc,
    Tail,
    Rmdir,
    Vi,
}
impl Program {
    pub fn named(s: &str, test: bool) -> Option<Self> {
        Some(match s {
            "cp" => Self::Cp,
            "mv" => Self::Mv,
            "touch" => Self::Touch,
            "more" => Self::More,
            "wc" => Self::Wc,
            "tail" => Self::Tail,
            "pwd" => Self::Pwd,
            "ls" => Self::Ls,
            "cat" => Self::Cat,
            "head" => Self::Head,
            "mkdir" => Self::Mkdir,
            "rm" => Self::Rm,
            "rmdir" => Self::Rmdir,
            "vi" => Self::Vi,
            "@probe" if test => Self::Probe,
            _ => return None,
        })
    }
}
pub enum Operand<'a> {
    Info(bool),
    Batch(crate::options::Options<'a>, Program),
    Edit(&'a str),
    Cwd,
    Create(&'a str),
    Remove(crate::options::Options<'a>, Program),
    Noop,
    Probe(&'a str),
}
pub fn operand<'a>(p: Program, args: &[&'a str]) -> Result<Operand<'a>, u64> {
    use crate::runtime::BAD_ARGUMENT;
    use Program::*;
    let options = crate::options::parse(p, args)?;
    if options.help || options.version {
        return Ok(Operand::Info(options.help));
    }
    let count = options.files.len();
    match p {
        Ls | Cat | Head | Tail | More | Wc | Cp | Mv | Touch => {
            if matches!(p, Cp | Mv) && count != 2 || p != Ls && count == 0 {
                return Err(BAD_ARGUMENT);
            }
            return Ok(Operand::Batch(options, p));
        }
        _ => {}
    }
    if matches!(p, Rm | Rmdir) {
        if p == Rm && count == 0 && options.has(b'f') {
            return Ok(Operand::Noop);
        }
        if count != 1 {
            return Err(BAD_ARGUMENT);
        }
        return Ok(Operand::Remove(options, p));
    }
    let args = options.files.as_slice();
    Ok(match (p, args) {
        (Pwd, []) => Operand::Cwd,
        (Vi, [name]) => Operand::Edit(name),
        (Mkdir, [path]) => Operand::Create(path),
        (Probe, [s]) => Operand::Probe(s),
        _ => return Err(BAD_ARGUMENT),
    })
}
