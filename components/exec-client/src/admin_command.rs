//! Grammaire binaire R1. Les commandes destructives n'ont aucun opcode.
//! Les sélecteurs ne sont jamais des capacités ou des chemins de fichiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selector<'a> {
    Id([u8; 16]),
    Name(&'a str),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Read<'a> {
    SelfIdentity,
    User(Selector<'a>),
    Group(Selector<'a>),
    UserGroup(Selector<'a>, u64),
    GroupMember(Selector<'a>, u64),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mutation<'a> {
    Create {
        name: &'a str,
        group: Selector<'a>,
        factor: [u8; 16],
    },
    Rename {
        user: Selector<'a>,
        name: &'a str,
    },
    Disable(Selector<'a>),
    Reactivate(Selector<'a>),
    CreateGroup(&'a str),
    AddMember {
        user: Selector<'a>,
        group: Selector<'a>,
    },
    RemoveMember {
        user: Selector<'a>,
        group: Selector<'a>,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command<'a> {
    Read(Read<'a>),
    Prepare(Mutation<'a>),
    Confirm {
        operation: [u8; 16],
        digest: [u8; 32],
    },
    Outcome([u8; 16]),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Invalid;
fn zero(b: &[u8]) -> Result<(), Invalid> {
    if b.iter().any(|b| *b != 0) {
        Err(Invalid)
    } else {
        Ok(())
    }
}
fn selector(b: &[u8]) -> Result<Selector<'_>, Invalid> {
    zero(&b[2..8])?;
    match b[0] {
        1 if b[1] == 16 && b[8..24] != [0; 16] => {
            zero(&b[24..])?;
            Ok(Selector::Id(b[8..24].try_into().unwrap()))
        }
        2 if (1..=32).contains(&b[1]) => {
            let n = b[1] as usize;
            zero(&b[8 + n..])?;
            let s = &b[8..8 + n];
            if s.iter()
                .any(|b| !b.is_ascii_lowercase() && !b.is_ascii_digit() && !b"-_".contains(b))
            {
                return Err(Invalid);
            }
            Ok(Selector::Name(
                core::str::from_utf8(s).map_err(|_| Invalid)?,
            ))
        }
        _ => Err(Invalid),
    }
}
fn name(s: Selector<'_>) -> Result<&str, Invalid> {
    match s {
        Selector::Name(n) => Ok(n),
        _ => Err(Invalid),
    }
}
pub fn decode(b: &[u8]) -> Result<Command<'_>, Invalid> {
    if b.len() != 128 || &b[..8] != b"NXACMD01" {
        return Err(Invalid);
    }
    zero(&b[9..16])?;
    let operation: [u8; 16] = b[16..32].try_into().unwrap();
    if matches!(b[8], 32 | 33) {
        if operation == [0; 16] {
            return Err(Invalid);
        }
        return if b[8] == 32 {
            zero(&b[64..])?;
            let digest = b[32..64].try_into().unwrap();
            if digest == [0; 32] {
                return Err(Invalid);
            }
            Ok(Command::Confirm { operation, digest })
        } else {
            zero(&b[32..])?;
            Ok(Command::Outcome(operation))
        };
    }
    let index = if matches!(b[8], 4 | 5) {
        zero(&b[24..32])?;
        u64::from_le_bytes(b[16..24].try_into().unwrap())
    } else {
        zero(&operation)?;
        0
    };
    let a = &b[32..72];
    let c = &b[72..112];
    let factor: [u8; 16] = b[112..].try_into().unwrap();
    if b[8] != 16 {
        zero(&factor)?;
    }
    Ok(match b[8] {
        1 => {
            zero(a)?;
            zero(c)?;
            Command::Read(Read::SelfIdentity)
        }
        2 | 3 | 4 | 5 => {
            zero(c)?;
            let s = selector(a)?;
            Command::Read(match b[8] {
                2 => Read::User(s),
                3 => Read::Group(s),
                4 => Read::UserGroup(s, index),
                _ => Read::GroupMember(s, index),
            })
        }
        16 => {
            if factor == [0; 16] {
                return Err(Invalid);
            }
            Command::Prepare(Mutation::Create {
                name: name(selector(a)?)?,
                group: selector(c)?,
                factor,
            })
        }
        17 => Command::Prepare(Mutation::Rename {
            user: selector(a)?,
            name: name(selector(c)?)?,
        }),
        18 | 19 => {
            zero(c)?;
            Command::Prepare(if b[8] == 18 {
                Mutation::Disable(selector(a)?)
            } else {
                Mutation::Reactivate(selector(a)?)
            })
        }
        20 => {
            zero(c)?;
            Command::Prepare(Mutation::CreateGroup(name(selector(a)?)?))
        }
        21 | 22 => Command::Prepare(if b[8] == 21 {
            Mutation::AddMember {
                user: selector(a)?,
                group: selector(c)?,
            }
        } else {
            Mutation::RemoveMember {
                user: selector(a)?,
                group: selector(c)?,
            }
        }),
        _ => return Err(Invalid),
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn packet(code: u8) -> [u8; 128] {
        let mut b = [0; 128];
        b[..8].copy_from_slice(b"NXACMD01");
        b[8] = code;
        b
    }
    fn named(b: &mut [u8], name: &[u8]) {
        b[0] = 2;
        b[1] = name.len() as u8;
        b[8..8 + name.len()].copy_from_slice(name);
    }
    #[test]
    fn only_explicit_non_destructive_operations_are_encodable() {
        for code in 0..=255u8 {
            let mut p = packet(code);
            match code {
                1 => (),
                2..=5 | 18 | 19 | 20 => named(&mut p[32..72], b"alice"),
                16 | 17 | 21 | 22 => {
                    named(&mut p[32..72], b"alice");
                    named(&mut p[72..112], b"group");
                    if code == 16 {
                        p[112..].fill(1);
                    }
                }
                32 | 33 => {
                    p[16..32].fill(1);
                    if code == 32 {
                        p[32..64].fill(2);
                    }
                }
                _ => (),
            }
            assert_eq!(
                decode(&p).is_ok(),
                matches!(code,1..=5|16..=22|32|33),
                "opcode {code}"
            );
        }
    }
    #[test]
    fn malformed_selectors_and_confirmation_are_not_permissive() {
        let mut p = packet(2);
        named(&mut p[32..72], b"alice");
        assert!(decode(&p).is_ok());
        for position in [0, 9, 15, 16, 31, 34, 39, 45, 71, 72, 112, 127] {
            let mut q = p;
            q[position] = 255;
            assert!(decode(&q).is_err(), "offset {position}");
        }
        p[40] = b'/';
        assert!(decode(&p).is_err());
        assert!(decode(&packet(32)).is_err());
        assert!(decode(&packet(33)).is_err());
    }
}
