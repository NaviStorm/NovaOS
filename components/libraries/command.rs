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
}
impl Program {
    pub fn named(s: &str, test: bool) -> Option<Self> {
        Some(match s {
            "pwd" => Self::Pwd,
            "ls" => Self::Ls,
            "cat" => Self::Cat,
            "head" => Self::Head,
            "mkdir" => Self::Mkdir,
            "rm" => Self::Rm,
            "@probe" if test => Self::Probe,
            _ => return None,
        })
    }
}
pub enum Operand<'a> {
    Cwd,
    Directory(&'a str, bool),
    Read(&'a str, Option<usize>),
    Create(&'a str),
    Remove(&'a str),
    Probe(&'a str),
}
pub fn operand<'a>(p: Program, args: &[&'a str]) -> Result<Operand<'a>, u64> {
    use crate::runtime::BAD_ARGUMENT;
    use Program::*;
    Ok(match (p, args) {
        (Pwd, []) => Operand::Cwd,
        (Ls, []) => Operand::Directory(".", false),
        (Ls, ["-l"]) => Operand::Directory(".", true),
        (Ls, ["-l", path]) => Operand::Directory(path, true),
        (Ls, [path]) if !path.starts_with('-') => Operand::Directory(path, false),
        (Cat, [path]) => Operand::Read(path, None),
        (Head, [path]) if *path != "-n" => Operand::Read(path, Some(10)),
        (Head, ["-n", n, path]) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
            Operand::Read(path, Some(n.parse().map_err(|_| BAD_ARGUMENT)?))
        }
        (Mkdir, [path]) => Operand::Create(path),
        (Rm, [path]) if !path.starts_with('-') => Operand::Remove(path),
        (Probe, [s]) => Operand::Probe(s),
        _ => return Err(BAD_ARGUMENT),
    })
}
