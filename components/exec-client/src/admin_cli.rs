//! Façades natives R2. Le parseur ne confère aucun droit ; chaque requête
//! traverse NXADMIN1 et les contrôles du registre. Aucune mutation réémise.
use crate::admin_command::{self as wire, Command};
use alloc::{format, string::String, vec::Vec};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Program {
    Whoami,
    Id,
    Groups,
    Useradd,
    Usermod,
    Groupadd,
}
impl Program {
    pub fn name(self) -> &'static str {
        match self {
            Self::Whoami => "whoami",
            Self::Id => "id",
            Self::Groups => "groups",
            Self::Useradd => "useradd",
            Self::Usermod => "usermod",
            Self::Groupadd => "groupadd",
        }
    }
    fn mutable(self) -> bool {
        matches!(self, Self::Useradd | Self::Usermod | Self::Groupadd)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Help,
    Version,
    Read {
        packet: [u8; 128],
        id_only: bool,
    },
    Prepare([u8; 128]),
    Confirm {
        prepare: [u8; 128],
        request: [u8; 16],
        digest: [u8; 32],
    },
    Outcome([u8; 16]),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Syntax;
fn packet(code: u8) -> [u8; 128] {
    let mut q = [0; 128];
    q[..8].copy_from_slice(b"NXACMD01");
    q[8] = code;
    q
}
fn hex<const N: usize>(s: &str) -> Result<[u8; N], Syntax> {
    if s.len() != 2 * N {
        return Err(Syntax);
    }
    let mut out = [0; N];
    fn digit(b: u8) -> Result<u8, Syntax> {
        match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            b'A'..=b'F' => Ok(b - b'A' + 10),
            _ => Err(Syntax),
        }
    }
    for (o, c) in out.iter_mut().zip(s.as_bytes().chunks_exact(2)) {
        *o = digit(c[0])? * 16 + digit(c[1])?;
    }
    if out == [0; N] { Err(Syntax) } else { Ok(out) }
}
fn selector(out: &mut [u8], name: &str, by_id: bool) -> Result<(), Syntax> {
    if by_id {
        out[0] = 1;
        out[1] = 16;
        out[8..24].copy_from_slice(&hex::<16>(name)?);
    } else {
        if name.is_empty()
            || name.len() > 32
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b))
        {
            return Err(Syntax);
        }
        out[0] = 2;
        out[1] = name.len() as u8;
        out[8..8 + name.len()].copy_from_slice(name.as_bytes());
    }
    Ok(())
}
/// Les options répétées et les paramètres métier d'une consultation d'issue
/// sont refusés avant tout IPC. `--` arrête réellement l'analyse des options.
pub fn parse(program: Program, args: &[&str]) -> Result<Action, Syntax> {
    if matches!(args, ["--help"] | ["man"]) {
        return Ok(Action::Help);
    }
    if args == ["--version"] {
        return Ok(Action::Version);
    }
    let mut positional = Vec::new();
    let mut flags = Vec::new();
    let mut ended = false;
    let mut prepare = false;
    let mut confirm = None;
    let mut request = None;
    let mut outcome = None;
    let mut subject = None;
    let mut group = None;
    let mut factor = None;
    let mut mutation = None;
    let mut id_only = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i];
        i += 1;
        if !ended && a == "--" {
            ended = true;
            continue;
        }
        if ended || !a.starts_with('-') {
            positional.push(a);
            continue;
        }
        if flags.contains(&a) {
            return Err(Syntax);
        }
        flags.push(a);
        let mut value = || {
            let v = args.get(i).copied().ok_or(Syntax)?;
            i += 1;
            Ok::<_, Syntax>(v)
        };
        match a {
            "--id" if program == Program::Whoami => id_only = true,
            "--subject-id"
                if matches!(program, Program::Id | Program::Groups | Program::Usermod) =>
            {
                subject = Some(value()?)
            }
            "--group" if matches!(program, Program::Useradd | Program::Groups) => {
                if group.is_some() {
                    return Err(Syntax);
                }
                group = Some((value()?, false));
            }
            "--group-id" if matches!(program, Program::Useradd | Program::Groups) => {
                if group.is_some() {
                    return Err(Syntax);
                }
                group = Some((value()?, true));
            }
            "--factor-reference" if program == Program::Useradd => {
                factor = Some(hex::<16>(value()?)?)
            }
            "--prepare" if program.mutable() => prepare = true,
            "--confirm" if program.mutable() => confirm = Some(hex::<32>(value()?)?),
            "--request" if program.mutable() => request = Some(hex::<16>(value()?)?),
            "--outcome" if program.mutable() => outcome = Some(hex::<16>(value()?)?),
            "--rename" | "--add-group" | "--remove-group" if program == Program::Usermod => {
                if mutation.is_some() {
                    return Err(Syntax);
                }
                mutation = Some((a, Some(value()?)));
            }
            "--disable" | "--enable" if program == Program::Usermod => {
                if mutation.is_some() {
                    return Err(Syntax);
                }
                mutation = Some((a, None));
            }
            _ => return Err(Syntax),
        }
    }
    if let Some(id) = outcome {
        return if flags.len() == 1 && positional.is_empty() {
            Ok(Action::Outcome(id))
        } else {
            Err(Syntax)
        };
    }
    if !program.mutable() {
        if let Some((name, by_id)) = group {
            if subject.is_some() || !positional.is_empty() { return Err(Syntax); }
            let mut q = packet(3);
            selector(&mut q[32..72], name, by_id)?;
            wire::decode(&q).map_err(|_| Syntax)?;
            return Ok(Action::Read { packet: q, id_only: false });
        }
        if subject.is_some() && !positional.is_empty()
            || positional.len() > 1
            || program == Program::Whoami && !positional.is_empty()
        {
            return Err(Syntax);
        }
        let mut q = packet(if subject.is_some() || !positional.is_empty() {
            2
        } else {
            1
        });
        if let Some(s) = subject {
            selector(&mut q[32..72], s, true)?;
        } else if let Some(s) = positional.first() {
            selector(&mut q[32..72], s, false)?;
        }
        wire::decode(&q).map_err(|_| Syntax)?;
        return Ok(Action::Read { packet: q, id_only });
    }
    if (prepare && (confirm.is_some() || request.is_some()))
        || (!prepare && (confirm.is_none() || request.is_none()))
    {
        return Err(Syntax);
    }
    let mut q = match program {
        Program::Useradd => {
            if positional.len() != 1 {
                return Err(Syntax);
            }
            let mut q = packet(16);
            selector(&mut q[32..72], positional[0], false)?;
            let (g, id) = group.ok_or(Syntax)?;
            selector(&mut q[72..112], g, id)?;
            q[112..].copy_from_slice(&factor.ok_or(Syntax)?);
            q
        }
        Program::Groupadd => {
            if positional.len() != 1 {
                return Err(Syntax);
            }
            let mut q = packet(20);
            selector(&mut q[32..72], positional[0], false)?;
            q
        }
        Program::Usermod => {
            let (op, arg) = mutation.ok_or(Syntax)?;
            let (name, id) = if let Some(s) = subject {
                if !positional.is_empty() {
                    return Err(Syntax);
                }
                (s, true)
            } else {
                if positional.len() != 1 {
                    return Err(Syntax);
                }
                (positional[0], false)
            };
            let code = match op {
                "--rename" => 17,
                "--disable" => 18,
                "--enable" => 19,
                "--add-group" => 21,
                _ => 22,
            };
            let mut q = packet(code);
            selector(&mut q[32..72], name, id)?;
            if let Some(a) = arg {
                selector(&mut q[72..112], a, false)?;
            }
            q
        }
        _ => return Err(Syntax),
    };
    // Défense supplémentaire contre la divergence parseur/contrat binaire.
    if !matches!(wire::decode(&q), Ok(Command::Prepare(_))) {
        q.fill(0);
        return Err(Syntax);
    }
    Ok(if prepare {
        Action::Prepare(q)
    } else {
        Action::Confirm {
            prepare: q,
            request: request.unwrap(),
            digest: confirm.unwrap(),
        }
    })
}
pub fn hex_text(bytes: &[u8]) -> String {
    let mut s = String::new();
    for b in bytes {
        use core::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}
/// Codes CLI distincts des statuts natifs. Une panne de dialogue après émission
/// est UNCERTAIN (20 CLI), jamais un succès ni une invitation à répéter.
pub fn exit_status(native: u64) -> u64 {
    match native {
        0 => 0,
        3 => 2,
        5 | 20 => 3,
        6 | 13 | 23 => 5,
        7 | 8 | 10 | 11 => 7,
        22 => 20,
        24 => 4,
        _ => 6,
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct Failure {
    pub native: u64,
    pub operation: Option<[u8; 16]>,
}
fn exchange(
    q: &[u8; 128],
    operation: Option<[u8; 16]>,
    call: &mut impl FnMut(&[u8; 128]) -> Result<[u8; 128], u64>,
) -> Result<[u8; 128], Failure> {
    let a = call(q).map_err(|native| Failure { native, operation })?;
    let native = u64::from_le_bytes(a[..8].try_into().unwrap());
    if native != 0 {
        return Err(Failure { native, operation });
    }
    Ok(a)
}
fn name(a: &[u8; 128]) -> Result<&str, Failure> {
    let n = a[24] as usize;
    if !(1..=32).contains(&n) || a[8..24] == [0; 16] {
        return Err(Failure {
            native: 12,
            operation: None,
        });
    }
    let name = core::str::from_utf8(&a[25..25 + n]).map_err(|_| Failure {
        native: 12,
        operation: None,
    })?;
    if !name
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"-_".contains(&b))
    {
        return Err(Failure {
            native: 12,
            operation: None,
        });
    }
    Ok(name)
}
fn prepared(a: &[u8; 128]) -> Result<([u8; 16], [u8; 32]), Failure> {
    let id = a[8..24].try_into().unwrap();
    let digest = a[24..56].try_into().unwrap();
    if id == [0; 16] || digest == [0; 32] {
        return Err(Failure {
            native: 12,
            operation: None,
        });
    }
    Ok((id, digest))
}
/// Résultat accumulé avant écriture : pas de listing tronqué présenté comme
/// complet, et pas de contenu administratif dans les diagnostics.
pub fn execute(
    program: Program,
    action: &Action,
    french: bool,
    mut call: impl FnMut(&[u8; 128]) -> Result<[u8; 128], u64>,
) -> Result<String, Failure> {
    match action {
        Action::Help => Ok(help(program, french)),
        Action::Version => Ok(format!(
            "{} 0.24.0 NovaOS NXADMIN1 experimental\n",
            program.name()
        )),
        Action::Read { packet: q, id_only } => {
            let a = exchange(q, None, &mut call)?;
            let n = name(&a)?;
            let id = &a[8..24];
            if program == Program::Whoami {
                return Ok(if *id_only {
                    format!("{}\n", hex_text(id))
                } else {
                    format!("{n}\n")
                });
            }
            let count = u64::from_le_bytes(a[72..80].try_into().unwrap());
            if count > 64 {
                return Err(Failure {
                    native: 6,
                    operation: None,
                });
            }
            if q[8] == 3 {
                let mut members = String::new();
                for i in 0..count {
                    let mut request = packet(5);
                    request[16..24].copy_from_slice(&i.to_le_bytes());
                    request[32] = 1; request[33] = 16;
                    request[40..56].copy_from_slice(id);
                    let member = exchange(&request, None, &mut call)?;
                    if member[80..112] != a[80..112] {
                        return Err(Failure { native: 24, operation: None });
                    }
                    if member[8..24] == [0;16] {
                        return Err(Failure { native: 12, operation: None });
                    }
                    if i > 0 { members.push(' '); }
                    members.push_str(&hex_text(&member[8..24]));
                }
                return Ok(format!("group={} name={n} members={members}\n", hex_text(id)));
            }
            let mut memberships = String::new();
            for i in 0..count {
                let mut q = packet(4);
                q[16..24].copy_from_slice(&i.to_le_bytes());
                q[32] = 1;
                q[33] = 16;
                q[40..56].copy_from_slice(id);
                let g = exchange(&q, None, &mut call)?;
                if g[80..112] != a[80..112] {
                    return Err(Failure {
                        native: 24,
                        operation: None,
                    });
                }
                if i > 0 {
                    memberships.push(' ');
                }
                memberships.push_str(name(&g)?);
                if program == Program::Id {
                    memberships.push('(');
                    memberships.push_str(&hex_text(&g[8..24]));
                    memberships.push(')');
                }
            }
            if program == Program::Groups {
                return Ok(format!("{memberships}\n"));
            }
            let status = match a[57] {
                1 => "ACTIVE",
                2 => "DISABLED",
                3 => "DELETED",
                _ => {
                    return Err(Failure {
                        native: 12,
                        operation: None,
                    });
                }
            };
            Ok(format!(
                "subject={} name={n} state={status} groups={memberships}\n",
                hex_text(id)
            ))
        }
        Action::Prepare(q) => {
            let a = exchange(q, None, &mut call)?;
            let (id, digest) = prepared(&a)?;
            Ok(format!(
                "request={} confirmation={} state=PREPARED\n",
                hex_text(&id),
                hex_text(&digest)
            ))
        }
        Action::Confirm {
            prepare,
            request,
            digest,
        } => {
            // La préparation est sans mutation et idempotente pour la même
            // session/tête/action. Vérifier les arguments répétés avant CONFIRM.
            let a = exchange(prepare, Some(*request), &mut call)?;
            if prepared(&a)? != (*request, *digest) {
                return Err(Failure {
                    native: 5,
                    operation: Some(*request),
                });
            }
            let mut q = packet(32);
            q[16..32].copy_from_slice(request);
            q[32..64].copy_from_slice(digest);
            let a = exchange(&q, Some(*request), &mut call)?;
            outcome(&a, *request)
        }
        Action::Outcome(id) => {
            let mut q = packet(33);
            q[16..32].copy_from_slice(id);
            let a = exchange(&q, Some(*id), &mut call)?;
            outcome(&a, *id)
        }
    }
}
fn outcome(a: &[u8; 128], id: [u8; 16]) -> Result<String, Failure> {
    if a[8..24] != id || a[32] != 1 || a[33..] != [0; 95] {
        return Err(Failure {
            native: 22,
            operation: Some(id),
        });
    }
    let generation = u64::from_le_bytes(a[24..32].try_into().unwrap());
    if generation == 0 {
        return Err(Failure {
            native: 12,
            operation: Some(id),
        });
    }
    Ok(format!(
        "request={} generation={generation} state=COMPLETE\n",
        hex_text(&id)
    ))
}
pub fn help(program: Program, fr: bool) -> String {
    let syntax = match program {
        Program::Whoami => "whoami [--id]",
        Program::Id => "id [--subject-id ID | NAME]",
        Program::Groups => "groups [--subject-id ID | NAME] | groups (--group NAME | --group-id ID)",
        Program::Useradd => "useradd --factor-reference REF (--group NAME | --group-id ID) NAME",
        Program::Usermod => {
            "usermod (--rename NAME | --disable | --enable | --add-group GROUP | --remove-group GROUP) (NAME | --subject-id ID)"
        }
        Program::Groupadd => "groupadd NAME",
    };
    let common = if fr {
        "Identifiants natifs stables ; aucun UID/GID POSIX.\n--help, man : cette notice ; --version : version ; -- : fin des options.\nStatuts CLI : 0 complet, 2 syntaxe, 3 refus, 4 conflit, 5 limite, 6 incohérence, 7 révoqué/expiré, 20 UNCERTAIN.\nMessages et diagnostics sur stderr ; résultats sur stdout. Aucun accès implicite aux données privées.\n"
    } else {
        "Stable native identities; no POSIX UID/GID.\n--help, man: this manual; --version: version; --: end of options.\nCLI statuses: 0 complete, 2 syntax, 3 denied, 4 conflict, 5 limit, 6 inconsistent state, 7 revoked/expired, 20 UNCERTAIN.\nDiagnostics go to stderr; results to stdout. No implicit access to private data.\n"
    };
    let mut out = format!("{syntax}\n{common}");
    if program.mutable() {
        out.push_str(if fr {
        "Mutation : arguments métier --prepare, puis mêmes arguments --confirm EMPREINTE --request ID.\n--outcome ID : consultation seule de l'issue, sans paramètres métier.\nUNCERTAIN : conserver ID et consulter l'issue, ne pas répéter la mutation.\nLa réactivation exige une nouvelle authentification ; anciennes sessions, capacités et tickets restent révoqués.\nAucune suppression de compte, de groupe ou de données ; aucune promotion administrative.\n"
    }else {
        "Mutation: business arguments --prepare, then identical arguments --confirm DIGEST --request ID.\n--outcome ID: read-only outcome lookup, without business arguments.\nUNCERTAIN: retain ID and query the outcome; never repeat the mutation.\nReactivation requires fresh authentication; old sessions, capabilities and tickets remain revoked.\nNo account, group or data deletion; no administrative promotion.\n"
    });
    }
    out
}
#[cfg(target_os = "none")]
pub fn main(program: Program) -> ! {
    use crate::admin::{AdministrativeLaunch, Conversation};
    use nova_cli::runtime as rt;
    let status = (|| -> Result<(), u64> {
        let launch = AdministrativeLaunch::current().map_err(exit_status)?;
        let action = match parse(program, &launch.streams.argv) {
            Ok(a) => a,
            Err(_) => {
                launch
                    .streams
                    .write_stderr(if launch.streams.french {
                        b"Syntaxe invalide ; consulter --help.\n"
                    } else {
                        b"Invalid syntax; see --help.\n"
                    })
                    .map_err(exit_status)?;
                return Err(2);
            }
        };
        // Aide/version : ne pas construire de conversation ni fermer un canal
        // métier ; la restitution normale de l'instance reste au noyau.
        let mut conversation = None;
        let result = execute(program, &action, launch.streams.french, |q| {
            conversation
                .get_or_insert_with(|| Conversation::new(&launch))
                .exchange(q)
        });
        match result {
            Ok(output) => launch
                .streams
                .write_stdout(output.as_bytes())
                .map_err(exit_status),
            Err(f) => {
                let label = if launch.streams.french {
                    "Échec administratif"
                } else {
                    "Administrative failure"
                };
                let mut line = format!(
                    "{label}: native={} status={}\n",
                    f.native,
                    exit_status(f.native)
                );
                if let Some(id) = f.operation {
                    line.push_str(&format!("request={} --outcome\n", hex_text(&id)));
                }
                launch
                    .streams
                    .write_stderr(line.as_bytes())
                    .map_err(exit_status)?;
                Err(exit_status(f.native))
            }
        }
    })()
    .err()
    .unwrap_or(0);
    rt::exit(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    const ID: &str = "01010101010101010101010101010101";
    const DIGEST: &str = "0202020202020202020202020202020202020202020202020202020202020202";
    fn prepared_reply() -> [u8; 128] {
        let mut a = [0; 128];
        a[8..24].fill(1);
        a[24..56].fill(2);
        a
    }
    #[test]
    fn group_view_uses_existing_filtered_protocol_and_rejects_mixed_heads() {
        let action = parse(Program::Groups, &["--group", "base"]).unwrap();
        let mut calls = 0;
        let mut read = |q: &[u8;128]| {
            calls += 1;
            let mut a = [0;128]; a[80..112].fill(9);
            match q[8] {
                3 => { a[8..24].fill(1); a[24]=4; a[25..29].copy_from_slice(b"base"); a[72]=2; }
                5 => { assert_eq!(&q[40..56], &[1;16]); a[8..24].fill(2+q[16]); }
                _ => panic!("unexpected opcode"),
            }
            Ok(a)
        };
        let out = execute(Program::Groups, &action, false, &mut read).unwrap();
        assert_eq!(out, format!("group={} name=base members={} {}\n",hex_text(&[1;16]),hex_text(&[2;16]),hex_text(&[3;16])));
        assert_eq!(calls,3);
        let error = execute(Program::Groups,&action,false,|q| {
            let mut a=[0;128];a[8..24].fill(1);a[24]=4;a[25..29].copy_from_slice(b"base");a[72]=1;
            a[80]=q[8]; Ok(a)
        }).unwrap_err();
        assert_eq!(error.native,24);
        for args in [alloc::vec!["--group","base","alice"],alloc::vec!["--group","base","--subject-id",ID],alloc::vec!["--group","base","--group-id",ID]] {
            assert_eq!(parse(Program::Groups,&args),Err(Syntax));
        }
    }
    #[test]
    fn group_view_never_masks_server_denial() {
        let action = parse(Program::Groups, &["--group-id", ID]).unwrap();
        assert_eq!(execute(Program::Groups,&action,false,|_|Err(5)).unwrap_err().native,5);
    }
    #[test]
    fn metadata_never_contacts_administration() {
        for p in [
            Program::Whoami,
            Program::Id,
            Program::Groups,
            Program::Useradd,
            Program::Usermod,
            Program::Groupadd,
        ] {
            for flag in ["--help", "man", "--version"] {
                for french in [false, true] {
                    let a = parse(p, &[flag]).unwrap();
                    let text =
                        execute(p, &a, french, |_| panic!("metadata performed IPC")).unwrap();
                    assert!(text.contains(p.name()));
                    assert!(text.ends_with('\n'));
                }
            }
        }
    }
    #[test]
    fn mutations_require_explicit_unambiguous_confirmation() {
        for args in [
            alloc::vec!["--disable", "bob"],
            alloc::vec!["--disable", "bob", "--yes"],
            alloc::vec!["--enable", "bob", "--force", "--prepare"],
            alloc::vec!["--disable", "--enable", "bob", "--prepare"],
            alloc::vec!["--disable", "bob", "--prepare", "--prepare"],
            alloc::vec!["--outcome", ID, "bob"],
            alloc::vec!["--disable", "bob", "--subject-id", ID, "--prepare"],
            alloc::vec!["--disable", "bob", "--confirm", DIGEST],
            alloc::vec!["--delete", "bob", "--prepare"],
            alloc::vec!["--admin", "bob", "--prepare"],
            alloc::vec![
                "--disable",
                "bob",
                "--prepare",
                "--confirm",
                DIGEST,
                "--request",
                ID
            ],
        ] {
            assert_eq!(parse(Program::Usermod, &args), Err(Syntax), "{args:?}");
        }
        assert_eq!(parse(Program::Whoami, &["--", "--id"]), Err(Syntax));
        assert_eq!(
            parse(
                Program::Useradd,
                &[
                    "--factor-reference",
                    ID,
                    "--group",
                    "g",
                    "--group-id",
                    ID,
                    "bob",
                    "--prepare"
                ]
            ),
            Err(Syntax)
        );
        assert!(parse(Program::Groupadd, &["--prepare", "--", "-staff"]).is_ok());
        assert!(parse(Program::Id, &["--subject-id", ID]).is_ok());
    }
    #[test]
    fn each_native_operation_has_an_exact_non_destructive_request() {
        let cases = [
            (
                Program::Useradd,
                alloc::vec!["--factor-reference", ID, "--group", "g", "bob", "--prepare"],
                16,
            ),
            (
                Program::Usermod,
                alloc::vec!["--rename", "robert", "bob", "--prepare"],
                17,
            ),
            (
                Program::Usermod,
                alloc::vec!["--disable", "bob", "--prepare"],
                18,
            ),
            (
                Program::Usermod,
                alloc::vec!["--enable", "bob", "--prepare"],
                19,
            ),
            (Program::Groupadd, alloc::vec!["staff", "--prepare"], 20),
            (
                Program::Usermod,
                alloc::vec!["--add-group", "staff", "bob", "--prepare"],
                21,
            ),
            (
                Program::Usermod,
                alloc::vec!["--remove-group", "staff", "bob", "--prepare"],
                22,
            ),
        ];
        for (p, args, opcode) in cases {
            let Action::Prepare(q) = parse(p, &args).unwrap() else {
                panic!()
            };
            assert_eq!(q[8], opcode);
            assert!(matches!(wire::decode(&q), Ok(Command::Prepare(_))));
            assert_eq!(q[16..32], [0; 16]);
        }
        for p in [Program::Whoami, Program::Id, Program::Groups] {
            assert!(matches!(parse(p, &[]).unwrap(), Action::Read { .. }));
        }
    }
    #[test]
    fn altered_confirmation_does_not_emit_a_mutation() {
        let a = parse(
            Program::Usermod,
            &["--disable", "bob", "--confirm", DIGEST, "--request", ID],
        )
        .unwrap();
        let mut calls = 0;
        let error = execute(Program::Usermod, &a, false, |q| {
            calls += 1;
            assert_eq!(q[8], 18);
            let mut r = prepared_reply();
            r[24] = 3;
            Ok(r)
        })
        .unwrap_err();
        assert_eq!(calls, 1);
        assert_eq!(error.native, 5);
        assert_eq!(error.operation, Some([1; 16]));
    }
    #[test]
    fn lost_confirmation_is_never_repeated_and_keeps_the_request() {
        let a = parse(
            Program::Usermod,
            &["--enable", "bob", "--confirm", DIGEST, "--request", ID],
        )
        .unwrap();
        let mut calls = alloc::vec![];
        let error = execute(Program::Usermod, &a, true, |q| {
            calls.push(q[8]);
            if q[8] == 19 {
                Ok(prepared_reply())
            } else {
                assert_eq!(q[16..32], [1; 16]);
                Err(22)
            }
        })
        .unwrap_err();
        assert_eq!(calls, [19, 32]);
        assert_eq!(error.operation, Some([1; 16]));
        assert_eq!(exit_status(error.native), 20);
        let outcome = parse(Program::Usermod, &["--outcome", ID]).unwrap();
        let mut n = 0;
        let error = execute(Program::Usermod, &outcome, false, |q| {
            n += 1;
            assert_eq!(q[8], 33);
            Err(22)
        })
        .unwrap_err();
        assert_eq!(n, 1);
        assert_eq!(error.native, 22);
    }
    #[test]
    fn durable_publication_without_completed_effects_is_not_success() {
        let a = Action::Outcome([1; 16]);
        let mut r = [0; 128];
        r[8..24].fill(1);
        r[24..32].copy_from_slice(&8u64.to_le_bytes());
        assert_eq!(
            execute(Program::Usermod, &a, false, |_| Ok(r))
                .unwrap_err()
                .native,
            22
        );
        r[32] = 1;
        assert_eq!(
            execute(Program::Usermod, &a, false, |_| Ok(r)).unwrap(),
            format!("request={ID} generation=8 state=COMPLETE\n")
        );
        r[8] = 3;
        assert!(execute(Program::Usermod, &a, false, |_| Ok(r)).is_err());
    }
    fn identity() -> [u8; 128] {
        let mut a = [0; 128];
        a[8..24].fill(1);
        a[24] = 5;
        a[25..30].copy_from_slice(b"alice");
        a[57] = 1;
        a[72] = 1;
        a[80..112].fill(4);
        a
    }
    #[test]
    fn reads_preserve_bytes_and_do_not_mix_registry_generations() {
        let a = identity();
        let mut g = identity();
        g[24] = 5;
        g[25..30].copy_from_slice(b"staff");
        g[8..24].fill(7);
        assert_eq!(
            execute(
                Program::Whoami,
                &parse(Program::Whoami, &[]).unwrap(),
                true,
                |_| Ok(a)
            )
            .unwrap(),
            "alice\n"
        );
        let parsed = parse(Program::Groups, &[]).unwrap();
        let mut codes = alloc::vec![];
        assert_eq!(
            execute(Program::Groups, &parsed, false, |q| {
                codes.push(q[8]);
                Ok(if q[8] == 1 { a } else { g })
            })
            .unwrap(),
            "staff\n"
        );
        assert_eq!(codes, [1, 4]);
        g[80] = 5;
        assert_eq!(
            execute(Program::Groups, &parsed, false, |q| Ok(if q[8] == 1 {
                a
            } else {
                g
            }))
            .unwrap_err()
            .native,
            24
        );
        let mut damaged = a;
        damaged[25] = 27;
        assert!(
            execute(
                Program::Whoami,
                &parse(Program::Whoami, &[]).unwrap(),
                false,
                |_| Ok(damaged)
            )
            .is_err()
        );
    }
}
