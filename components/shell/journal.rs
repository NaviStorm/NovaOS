//! Client du journal natif. La paire d'endpoints identifie la session côté service.
use super::services::{state, wire::service_id};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use nova_cli::{console, protocol as fs, runtime as rt, tty};

#[path = "../../novaOS/user/services/journal_wire.rs"]
pub(super) mod j;

const LEVELS: [&str; 6] = ["TRACE", "DEBUG", "INFO", "WARN", "ERROR", "CRITICAL"];
const CATEGORIES: [&str; 7] = [
    "",
    "SYSTEM",
    "LIFECYCLE",
    "CONFIG",
    "DEPENDENCY",
    "CAPABILITY",
    "SERVICE",
];
const CODES: [&str; 18] = [
    "",
    "BOOT",
    "SHUTDOWN",
    "STATE",
    "DISABLED",
    "ENABLED",
    "CONFIG_ERROR",
    "DEPENDENCY_WAIT",
    "WITHDRAWN",
    "RESTART",
    "DROPPED",
    "NOTICE",
    "ONLINE",
    "CAP_RESTRICTED",
    "CAP_RESTORED",
    "CAP_DENIED",
    "CAP_APPLIED",
    "CAP_MISSING",
];
const MAX_DISPLAY: usize = 128;
const HELP: &str = "\
journal list | status
journal show <service|self> [options]
journal tail <service|self> [options] [--follow]
journal boot [--service <service>] [options]
Options : --level TRACE|DEBUG|INFO|WARN|ERROR|CRITICAL, --boot <32 hex>, --limit 1..128
journal emit <0..255> | rotate | retain <1..8> | purge
Suivi : q, Échap ou Ctrl+C pour quitter.
";
static TAG: AtomicU64 = AtomicU64::new(0);

fn request(mut frame: j::Frame) -> rt::Result<j::Frame> {
    let tx = rt::config(18);
    let rx = rt::config(19);
    if tx == 0 || rx == 0 {
        return Err(rt::REFUSED);
    }
    let op = frame[1];
    let mutation = matches!(op, j::ROTATE | j::RETAIN | j::PURGE | j::SELF_NOTICE);
    let tag = 1 + TAG
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| rt::LIMIT)?;
    if op == j::SELF_NOTICE {
        frame[3] = tag;
    }
    rt::send_for(tx, tag, &j::encode(&frame), 15000)?;
    // Après le rendez-vous, la disparition de la réponse ne prouve plus l'échec de la mutation.
    let lost = |e| if mutation { fs::UNCERTAIN } else { e };
    for _ in 0..=8 {
        let msg = rt::receive_for(rx, 15000).map_err(lost)?;
        if msg.kind != 1 || msg.len != 128 || msg.tag == 0 || msg.tag > tag {
            return Err(lost(rt::BAD_ARGUMENT));
        }
        if msg.tag < tag {
            continue;
        }
        let reply = j::decode(&msg.payload).ok_or_else(|| lost(rt::BAD_ARGUMENT))?;
        if reply[0] != j::VERSION || reply[1] != op {
            return Err(lost(rt::BAD_VERSION));
        }
        return Ok(reply);
    }
    Err(lost(rt::LIMIT))
}

fn checked(frame: j::Frame) -> rt::Result<j::Frame> {
    let f = request(frame)?;
    if f[2] == rt::OK { Ok(f) } else { Err(f[2]) }
}

fn print_event(e: &j::Event) -> rt::Result<()> {
    let code = j::code(e);
    let src = match j::origin(e) {
        j::MANAGER => "gestionnaire",
        j::PRODUCER => "producteur",
        j::LOGGER => "journal",
        _ => return Err(fs::CORRUPT),
    };
    let clock = if j::quality(e) == j::MONOTONIC {
        "monotone"
    } else {
        "inconnue"
    };
    let transition = if code == j::STATE { state(e[10])? } else { "-" };
    console::write(
        tty::OUTPUT,
        &alloc::format!(
            "seq={} eid={} boot={:016x}{:016x} service={} instance={}/{} émetteur={}/{} source={src} niveau={} catégorie={} code={} état={transition} horloge={clock} ms={} données={},{}\n",
            e[1],
            e[0],
            e[2],
            e[3],
            j::service(e),
            e[4],
            e[5],
            e[6],
            e[7],
            LEVELS[j::level(e) as usize],
            CATEGORIES[j::category(e) as usize],
            CODES[code as usize],
            e[9],
            e[10],
            e[11]
        ),
    )
}

#[derive(Default)]
struct Filter {
    service: u64,
    level: u64,
    boot: [u64; 2],
}

impl Filter {
    fn event(&self, op: u64, cursor: u64, reverse: bool) -> rt::Result<(u64, Option<j::Event>)> {
        let mut f = j::request(op);
        f[3..9].copy_from_slice(&[
            cursor,
            self.service,
            self.level,
            self.boot[0],
            self.boot[1],
            u64::from(reverse),
        ]);
        let f = request(f)?;
        match f[2] {
            rt::WOULD_BLOCK => Ok((f[3], None)),
            rt::OK => {
                let e: j::Event = f[4..].try_into().map_err(|_| rt::BAD_ARGUMENT)?;
                if !j::valid(&e)
                    || e[1] == 0
                    || (cursor != 0 && ((reverse && e[1] > cursor) || (!reverse && e[1] < cursor)))
                    || (reverse && f[3] != j::END && f[3] >= e[1])
                    || (!reverse && f[3] <= e[1])
                {
                    return Err(fs::CORRUPT);
                }
                Ok((f[3], Some(e)))
            }
            code => Err(code),
        }
    }
}

fn list() -> rt::Result<()> {
    // Borne haute prise une fois pour que les publications concurrentes ne prolongent pas la liste.
    let status = checked(j::request(j::STATUS))?;
    if status[13] != 0 {
        return Err(rt::DENIED);
    }
    let end = status[4];
    let mut cursor = status[3];
    let mut services: Vec<(u64, usize)> = Vec::new();
    let filter = Filter::default();
    while cursor < end {
        let (next, e) = filter.event(j::NEXT, cursor, false)?;
        let Some(e) = e else { break };
        if e[1] >= end {
            break;
        }
        let id = j::service(&e);
        match services.binary_search_by_key(&id, |&(service, _)| service) {
            Ok(i) => services[i].1 += 1,
            Err(i) => {
                services.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
                services.insert(i, (id, 1));
            }
        }
        cursor = next;
    }
    for (id, count) in services {
        console::write(
            tty::OUTPUT,
            &alloc::format!("service={id} événements={count}\n"),
        )?;
    }
    Ok(())
}

fn service(s: &str) -> rt::Result<u64> {
    if s == "self" {
        return Ok(j::SELF);
    }
    service_id(s)
        .filter(|n| *n <= u64::from(u16::MAX))
        .ok_or(fs::NOT_FOUND)
}

fn boot_id(s: &str) -> rt::Result<[u64; 2]> {
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(rt::BAD_ARGUMENT);
    }
    let part = |s| u64::from_str_radix(s, 16).map_err(|_| rt::BAD_ARGUMENT);
    let id = [part(&s[..16])?, part(&s[16..])?];
    if id == [0; 2] {
        Err(rt::BAD_ARGUMENT)
    } else {
        Ok(id)
    }
}

fn quit() -> rt::Result<bool> {
    match rt::value(rt::call(
        tty::CONSOLE,
        tty::SCREEN.into(),
        tty::READ_TIMED,
        1,
    )) {
        Ok(3 | 27 | 113) => Ok(true),
        Ok(_) | Err(rt::TIMED_OUT) => Ok(false),
        Err(e) => Err(e),
    }
}

fn display(verb: &str, args: &[&str]) -> rt::Result<()> {
    let tail = verb == "tail";
    let mut filter = Filter::default();
    let mut limit = if tail { 10 } else { 32 };
    let mut follow = false;
    let mut opts = args.iter().copied();
    if verb != "boot" {
        filter.service = service(opts.next().ok_or(rt::BAD_ARGUMENT)?)?;
    }
    while let Some(key) = opts.next() {
        let mut arg = || opts.next().ok_or(rt::BAD_ARGUMENT);
        match key {
            "--level" => {
                let level = arg()?;
                filter.level = LEVELS
                    .iter()
                    .position(|name| name.eq_ignore_ascii_case(level))
                    .ok_or(rt::BAD_ARGUMENT)? as u64;
            }
            "--boot" => filter.boot = boot_id(arg()?)?,
            "--service" if verb == "boot" => filter.service = service(arg()?)?,
            "--limit" => {
                limit = arg()?.parse::<usize>().map_err(|_| rt::BAD_ARGUMENT)?;
                if !(1..=MAX_DISPLAY).contains(&limit) {
                    return Err(rt::BAD_ARGUMENT);
                }
            }
            "--follow" if tail => follow = true,
            _ => return Err(rt::BAD_ARGUMENT),
        }
    }
    if verb == "boot" {
        let f = checked(j::request(j::STATUS))?;
        if f[13] != 0 {
            return Err(rt::DENIED);
        }
        if filter.boot == [0; 2] {
            filter.boot = [f[10], f[11]];
        }
    }

    let mut cursor = 0;
    let mut future = 0;
    let mut recent = Vec::new();
    for i in 0..limit {
        let (next, entry) = filter.event(j::NEXT, cursor, tail)?;
        let Some(entry) = entry else {
            if i == 0 && next != j::END {
                future = next;
            }
            break;
        };
        if tail {
            if i == 0 {
                future = entry[1].checked_add(1).ok_or(rt::LIMIT)?;
            }
            recent.try_reserve(1).map_err(|_| rt::NO_MEMORY)?;
            recent.push(entry);
        } else {
            print_event(&entry)?;
        }
        cursor = next;
        if tail && cursor == j::END {
            break;
        }
    }
    for e in recent.iter().rev() {
        print_event(e)?;
    }
    if follow {
        console::write(tty::OUTPUT, "suivi du journal (q pour quitter)\n")?;
        while !quit()? {
            let (next, e) = filter.event(j::WATCH, future, false)?;
            if let Some(e) = e {
                print_event(&e)?;
            }
            future = next;
        }
    }
    Ok(())
}

fn admin(args: &[&str]) -> rt::Result<()> {
    match args {
        ["help" | "--help"] => console::write(tty::OUTPUT, HELP),
        ["list"] => list(),
        ["status"] => {
            let f = checked(j::request(j::STATUS))?;
            if f[13] == 1 {
                return console::write(tty::OUTPUT, &alloc::format!("révision={}\n", f[7]));
            }
            console::write(
                tty::OUTPUT,
                &alloc::format!(
                    "boot={:016x}{:016x} premier={} suivant={} segments={} rétention={} révision={} quota={} octets={}\n",
                    f[10],
                    f[11],
                    f[3],
                    f[4],
                    f[5],
                    f[6],
                    f[7],
                    f[8],
                    f[9]
                ),
            )
        }
        [verb @ ("show" | "boot" | "tail"), options @ ..] => display(verb, options),
        ["emit", value] => {
            let mut f = j::request(j::SELF_NOTICE);
            f[4] = value.parse::<u8>().map_err(|_| rt::BAD_ARGUMENT)?.into();
            let reply = checked(f)?;
            console::write(
                tty::OUTPUT,
                &alloc::format!("émission confirmée eid={}\n", reply[3]),
            )
        }
        ["rotate"] | ["purge"] | ["retain", _] => {
            let (op, n) = match args {
                ["rotate"] => (j::ROTATE, 0),
                ["purge"] => (j::PURGE, 0),
                ["retain", n] => (j::RETAIN, n.parse::<u64>().map_err(|_| rt::BAD_ARGUMENT)?),
                _ => unreachable!(),
            };
            let mut f = j::request(op);
            f[3] = checked(j::request(j::STATUS))?[7];
            f[4] = n;
            let reply = checked(f)?;
            console::write(
                tty::OUTPUT,
                &alloc::format!("{} confirmé révision={}\n", args[0], reply[3]),
            )
        }
        _ => Err(rt::BAD_ARGUMENT),
    }
}

pub fn command(args: &[&str]) -> rt::Result<u64> {
    match admin(args) {
        Ok(()) => Ok(0),
        Err(e) => {
            let msg = match e {
                fs::UNCERTAIN => "UNCERTAIN, relire le journal et sa révision avant toute décision",
                rt::REVOKED => "curseur expiré ou capacité révoquée",
                rt::REFUSED => "journal indisponible",
                fs::CORRUPT => "journal corrompu",
                fs::FULL => "volume plein",
                rt::TIMED_OUT => "délai dépassé",
                _ => console::reason(e),
            };
            console::write(tty::ERROR, &alloc::format!("journal: {msg} ({e})\n"))?;
            Ok(e)
        }
    }
}
