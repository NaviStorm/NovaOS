//! Client de contrôle des services et entrée historique de maintenance de nsh.
use core::sync::atomic::{AtomicU64, Ordering};
use nova_cli::{console, runtime as rt, tty};
#[path = "../../novaOS/user/services/protocol.rs"]
mod wire;

static TAG: AtomicU64 = AtomicU64::new(0);

fn request(op: u64, id: u64, arg: u64) -> rt::Result<wire::Frame> {
    let persistent = matches!(op, wire::ENABLE | wire::DISABLE);
    let slot = match op {
        wire::ENABLE | wire::DISABLE => 16,
        wire::START | wire::STOP | wire::RESTART => 14,
        _ => 11,
    };
    let (tx, rx) = (rt::config(slot), rt::config(slot + 1));
    if tx == 0 || rx == 0 {
        return Err(rt::DENIED);
    }
    let prev = TAG
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .map_err(|_| rt::LIMIT)?;
    let tag = prev + 1;
    rt::send_for(tx, tag, &wire::encode(&wire::request(op, id, arg)), 15000)?;
    // Une réponse perdue après la remise d'une mutation laisse son résultat inconnu.
    // Une lecture ultérieure peut rencontrer l'acquittement tardif sur la même paire.
    let missing = |e| {
        if persistent {
            nova_cli::protocol::UNCERTAIN
        } else {
            e
        }
    };
    for _ in 0..=8 {
        let reply = rt::receive_for(rx, 15000).map_err(missing)?;
        if reply.kind != 1 || reply.len != 128 || reply.tag == 0 || reply.tag > tag {
            return Err(missing(rt::BAD_ARGUMENT));
        }
        if reply.tag < tag {
            continue;
        }
        let frame = wire::decode(&reply.payload).ok_or_else(|| missing(rt::BAD_ARGUMENT))?;
        if frame[0] != wire::VERSION {
            return Err(missing(rt::BAD_VERSION));
        }
        if id != 0 && frame[2] != id {
            return Err(missing(rt::BAD_ARGUMENT));
        }
        if frame[1] != rt::OK {
            return Err(frame[1]);
        }
        return Ok(frame);
    }
    Err(missing(rt::LIMIT))
}

fn show(frame: &wire::Frame) -> rt::Result<()> {
    console::write(tty::OUTPUT, &alloc::format!(
        "service={} etat={} essai={} pid={} instance={:016x}/{:016x}/{:016x}/{:016x} valeur={} retraits={}\n",
        frame[2], frame[3], frame[4], frame[5], frame[6], frame[7], frame[8], frame[9], frame[10], frame[14]))
}

pub fn run(args: &[&str]) -> rt::Result<()> {
    fn number(s: &str) -> rt::Result<u64> { s.parse().map_err(|_| rt::BAD_ARGUMENT) }
    if args == ["list"] {
        for index in 0..=12 {
            match request(wire::LIST, 0, index) {
                Ok(data) if index < 12 => show(&data)?,
                Err(rt::REFUSED) => return Ok(()),
                Ok(_) => return Err(rt::LIMIT),
                Err(e) => return Err(e),
            }
        }
        return Ok(());
    }
    let (op, id, arg) = match args {
        ["status", id] => (wire::STATUS, *id, 0),
        ["info", id] => (wire::QUERY, *id, 0),
        ["start", id] => (wire::START, *id, 0),
        ["stop", id] => (wire::STOP, *id, 0),
        ["restart", id] => (wire::RESTART, *id, 0),
        ["dep", id, index] => (wire::DEP, *id, number(index)?),
        _ => return Err(rt::BAD_ARGUMENT),
    };
    let frame = request(op, number(id)?, arg)?;
    if op == wire::DEP {
        let kind = match frame[11] {
            0 => "optionnelle",
            1 => "obligatoire",
            _ => return Err(rt::REFUSED),
        };
        return console::write(tty::OUTPUT, &alloc::format!(
            "service={} dependance={} {}\n", frame[2], frame[10], kind));
    }
    show(&frame)
}

const HELP: &str = "\
service list\n\
service status|dependencies|policy|info <nom>\n\
service start|stop|restart <nom>\n\
service enable|disable <nom>\n\
enable et disable enregistrent le choix pour le prochain démarrage.\n";

fn state(value: u64) -> rt::Result<&'static str> {
    Ok(match value {
        0 => "UNREGISTERED",
        wire::REGISTERED => "REGISTERED",
        wire::WAITING => "WAITING",
        wire::STARTING => "STARTING",
        wire::READY => "READY",
        wire::RUNNING => "RUNNING",
        wire::STOPPING => "STOPPING",
        wire::STOPPED => "STOPPED",
        wire::FAILED => "FAILED",
        wire::RESTARTING => "RESTARTING",
        _ => return Err(rt::BAD_ARGUMENT),
    })
}

fn autostart(value: u64) -> rt::Result<&'static str> {
    match value {
        0 => Ok("disabled"),
        1 => Ok("enabled"),
        _ => Err(rt::BAD_ARGUMENT),
    }
}

fn status(frame: &wire::Frame) -> rt::Result<()> {
    let id = frame[2];
    let name = wire::service_name(id).unwrap_or("sonde");
    let pending = match frame[8] {
        0 => "no",
        1 => "yes",
        _ => return Err(rt::BAD_ARGUMENT),
    };
    console::write(
        tty::OUTPUT,
        &alloc::format!(
            "service={name} id={id}\nCurrent state: {}\nAutostart: {}\nEffective generation: {}\nStored generation: {}\nPending restart: {pending}\n",
            state(frame[3])?,
            autostart(frame[10])?,
            frame[4],
            frame[12]
        ),
    )?;
    if frame[5] != 0 {
        console::write(
            tty::OUTPUT,
            &alloc::format!("Cause: dépendance {} indisponible\n", frame[5]),
        )?;
    }
    if frame[15] != rt::OK {
        return Err(frame[15]);
    }
    Ok(())
}

fn admin(args: &[&str]) -> rt::Result<()> {
    if args == ["--help"] || args == ["help"] {
        return console::write(tty::OUTPUT, HELP);
    }
    if args == ["list"] {
        for index in 0..=12 {
            match request(wire::LIST, 0, index) {
                Err(rt::REFUSED) => return Ok(()),
                Ok(f) if index < 12 => console::write(
                    tty::OUTPUT,
                    &alloc::format!(
                        "service={} id={} etat={}\n",
                        wire::service_name(f[2]).unwrap_or("sonde"),
                        f[2],
                        state(f[3])?
                    ),
                )?,
                Ok(_) => return Err(rt::LIMIT),
                Err(e) => return Err(e),
            }
        }
        return Ok(());
    }
    let [verb, name] = args else {
        return Err(rt::BAD_ARGUMENT);
    };
    let id = wire::service_id(name).ok_or(nova_cli::protocol::NOT_FOUND)?;
    let name = wire::service_name(id).unwrap_or("sonde");
    match *verb {
        "status" => status(&request(wire::SETTINGS, id, 0)?),
        "policy" => {
            let f = request(wire::SETTINGS, id, 0)?;
            if f[15] != rt::OK {
                return Err(f[15]);
            }
            let restart = match f[13] {
                0 => "never",
                1 => "failure",
                2 => "always",
                _ => return Err(rt::BAD_ARGUMENT),
            };
            console::write(
                tty::OUTPUT,
                &alloc::format!(
                    "service={name} id={id} restart={restart} retries={}\n",
                    f[14]
                ),
            )
        }
        "start" | "stop" | "restart" => {
            let op = match *verb {
                "start" => wire::START,
                "stop" => wire::STOP,
                _ => wire::RESTART,
            };
            let f = request(op, id, 0)?;
            console::write(
                tty::OUTPUT,
                &alloc::format!(
                    "service={name} id={id} demande={verb} etat={}\n",
                    state(f[3])?
                ),
            )
        }
        "enable" | "disable" => {
            let current = request(wire::SETTINGS, id, 0)?;
            if current[15] != rt::OK {
                return Err(current[15]);
            }
            let op = if *verb == "enable" {
                wire::ENABLE
            } else {
                wire::DISABLE
            };
            let f = request(op, id, current[12])?;
            console::write(
                tty::OUTPUT,
                &alloc::format!(
                    "service={name} id={id} enregistré={} génération={}\n",
                    autostart(f[10])?,
                    f[12]
                ),
            )
        }
        "dependencies" => {
            request(wire::SETTINGS, id, 0)?;
            for index in 0..=12 {
                match request(wire::DEP, id, index) {
                    Err(rt::REFUSED) if index == 0 => {
                        return console::write(tty::OUTPUT, "aucune dépendance\n");
                    }
                    Err(rt::REFUSED) => return Ok(()),
                    Ok(f) if index < 12 => {
                        let relation = match f[11] {
                            0 => "optionnelle",
                            1 => "obligatoire",
                            _ => return Err(rt::BAD_ARGUMENT),
                        };
                        console::write(
                            tty::OUTPUT,
                            &alloc::format!(
                                "dépendance={} id={} {relation}\n",
                                wire::service_name(f[10]).unwrap_or("sonde"),
                                f[10]
                            ),
                        )?;
                    }
                    Ok(_) => return Err(rt::LIMIT),
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        }
        "info" => {
            let f = request(wire::QUERY, id, 0)?;
            console::write(
                tty::OUTPUT,
                &alloc::format!("service={name} id={id} valeur={}\n", f[10]),
            )
        }
        _ => Err(rt::BAD_ARGUMENT),
    }
}

pub fn command(args: &[&str]) -> rt::Result<u64> {
    use nova_cli::protocol as w;
    match admin(args) {
        Ok(()) => Ok(0),
        Err(e) => {
            let reason = match e {
                w::UNCERTAIN => "publication incertaine. Consulter service status",
                w::BUSY => "configuration modifiée ou occupée",
                w::FULL => "volume plein",
                w::CORRUPT => "configuration corrompue",
                rt::BAD_ARGUMENT => "argument invalide (service --help)",
                rt::REFUSED => "service ou transition indisponible",
                rt::TIMED_OUT => "délai dépassé",
                _ => console::reason(e),
            };
            console::write(tty::ERROR, &alloc::format!("service: {reason} ({e})\n"))?;
            Ok(e)
        }
    }
}
