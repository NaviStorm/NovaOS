//! Client d’exploitation du banc P9. Chaque résultat vient d’un véritable échange IPC.
#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "../abi/files.rs"]
mod protocol;
#[allow(dead_code)]
#[path = "storage/fileapi.rs"]
mod fileapi;
#[allow(dead_code)]
#[path = "storage/runtime.rs"]
mod runtime;
#[path = "services/protocol.rs"]
mod svc;
#[allow(dead_code)]
#[path = "../abi/instances.rs"]
mod instances;
#[allow(dead_code)]
#[path = "../abi/service_process.rs"]
mod service_process;

use runtime as rt;
const UNKNOWN_CONTROL_OPERATION: u64 = 12345;
fn log(line: &[u8]) {
    rt::call(rt::DEBUG_WRITE, line.as_ptr() as u64, line.len() as u64, 0);
}

fn require(ok: bool) -> rt::Result<()> {
    if ok { Ok(()) } else { Err(rt::REFUSED) }
}

struct Control {
    tx: u64,
    rx: u64,
    tag: u64,
}

impl Control {
    fn submit(&mut self, op: u64, id: u64, arg: u64) -> rt::Result<()> {
        self.tag = self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        rt::send_for(self.tx, self.tag, &svc::encode(&svc::request(op, id, arg)), 15000)
    }

    fn call(&mut self, op: u64, id: u64, arg: u64) -> rt::Result<svc::Frame> {
        self.submit(op, id, arg)?;
        let msg = rt::receive_for(self.rx, 15000)?;
        require(msg.kind == 1 && msg.tag == self.tag && msg.len == 128)?;
        let response = svc::decode(&msg.payload).ok_or(rt::BAD_ARGUMENT)?;
        require(response[0] == svc::VERSION)?;
        if response[1] != rt::OK { return Err(response[1]); }
        Ok(response)
    }

    fn until(&mut self, id: u64, predicate: impl Fn(&svc::Frame) -> bool) -> rt::Result<svc::Frame> {
        for _ in 0..5000 {
            let state = self.call(svc::STATUS, id, 0)?;
            if predicate(&state) { return Ok(state); }
            // En l’absence de requête en vol, cet endpoint privé doit rester silencieux.
            match rt::receive_for(self.rx, 4) {
                Err(rt::TIMED_OUT) => (),
                Err(e) => return Err(e),
                Ok(_) => return Err(rt::REFUSED),
            }
        }
        Err(rt::TIMED_OUT)
    }

    fn state(&mut self, id: u64, expected: u64) -> rt::Result<svc::Frame> {
        self.until(id, |reply| reply[3] == expected)
    }

    fn boot_order(&mut self) -> rt::Result<()> {
        let mut started = 0;
        let mut ready = 0;
        for seq in 1..=32 {
            let event = self.call(svc::LOG, 0, seq)?;
            require(event[15] == seq)?;
            if [1, 2, 9].contains(&event[2]) {
                if event[3] == svc::STARTING { started |= 1u16 << event[2]; }
                if event[3] == svc::READY {
                    require(started == (1 << 1 | 1 << 2 | 1 << 9))?;
                    ready |= 1u16 << event[2];
                }
                if ready == started && ready != 0 { return Ok(()); }
            }
        }
        Err(rt::REFUSED)
    }

    fn storage(&mut self) -> rt::Result<()> {
        let first = self.state(9, svc::RUNNING)?;
        require(self.call(svc::QUERY, 9, 0)?[10] == svc::NATIVE_INFO)?;
        self.call(svc::STOP, 9, 0)?;
        self.state(9, svc::STOPPED)?;
        require(self.call(svc::QUERY, 9, 0).is_err())?;
        self.call(svc::START, 9, 0)?;
        let next = self.state(9, svc::RUNNING)?;
        require(first[11] == 1 && next[11] == 2)?;
        require(first[4] < next[4] && first[5] != next[5] && first[5] != 0)?;
        require(first[6..8] == next[6..8] && first[6..8] != [0, 0])?;
        require(first[8..10] != next[8..10] && next[14] > first[14])?;
        require(self.call(svc::QUERY, 9, 0)?[10] == svc::NATIVE_INFO)?;
        log(b"SVC-P3 stockage INFO arret nouvelle-identite anciens-canaux-retires INFO OK");
        Ok(())
    }

    fn stale_ready(&mut self) -> rt::Result<()> {
        self.call(svc::REGISTER, 7, 0)?;
        self.call(svc::REGISTER, 8, 0)?;
        self.call(svc::START, 7, 0)?;
        self.until(8, |r| r[3] == svc::STARTING && r[12] >= 3 && r[13] == 0)?;
        let waiting = self.call(svc::STATUS, 7, 0)?;
        require(waiting[3] == svc::WAITING && waiting[5] == 0 && waiting[13] == 0)?;
        log(b"SVC-P3 READY ancien refuse dependant WAITING sans PID");
        self.call(svc::INJECT, 8, 6)?;
        let current = self.state(8, svc::RUNNING)?;
        require(current[12] > 0 && current[13] == 1)?;
        self.state(7, svc::RUNNING)?;
        let required = self.call(svc::DEP, 7, 0)?;
        let optional = self.call(svc::DEP, 7, 1)?;
        require(required[10..12] == [8, 1] && optional[10..12] == [2, 0])?;
        let delayed = self.call(svc::QUERY, 7, 0)?;
        require(delayed[10] == 0 && delayed[15] > 0)?;
        log(b"SVC-P3 READY courant admis dependant RUNNING");
        Ok(())
    }

    fn failures(&mut self) -> rt::Result<()> {
        for id in [3, 4, 5, 6] {
            self.call(svc::REGISTER, id, 0)?;
        }
        require(self.call(svc::REGISTER, 99, 0).is_err())?;
        for invalid in [100, 101] {
            require(self.call(svc::REGISTER, invalid, 0).err() == Some(rt::BAD_ARGUMENT))?;
            require(self.call(svc::STATUS, invalid, 0).err() == Some(rt::REFUSED))?;
        }
        require(self.call(UNKNOWN_CONTROL_OPERATION, 9, 0).is_err())?;
        self.call(svc::START, 3, 0)?;
        let before = self.state(3, svc::RUNNING)?;
        self.call(svc::STOP, 1, 0)?;
        self.state(1, svc::STOPPED)?;
        // Relire les transitions produites par les deux vrais processus.
        let mut entry = self.call(svc::LOG, 0, 0)?;
        let mut consumer_stopped = false;
        loop {
            if entry[2] == 3 && entry[3] == svc::STOPPED { consumer_stopped = true; }
            if entry[2] == 1 && entry[3] == svc::STOPPING { break; }
            entry = self.call(svc::LOG, 0, entry[15].checked_add(1).ok_or(rt::LIMIT)?)?;
        }
        require(consumer_stopped)?;
        let dependent = self.call(svc::STATUS, 3, 0)?;
        require(dependent[3] != svc::RUNNING && dependent[3] != svc::READY)?;
        self.call(svc::START, 1, 0)?;
        self.state(1, svc::RUNNING)?;
        self.call(svc::START, 3, 0)?;
        let after = self.state(3, svc::RUNNING)?;
        require(before[8..10] != after[8..10])?;
        log(b"SVC-P3 dependance obligatoire perdue puis retablie");
        for (op, expected) in [(1, svc::STOPPED), (3, svc::FAILED), (4, svc::FAILED)] {
            self.call(svc::INJECT, 3, op)?;
            self.state(3, expected)?;
            self.call(svc::START, 3, 0)?;
            self.state(3, svc::RUNNING)?;
        }
        log(b"SVC-P3 sortie perte de canal et silence detectes");

        self.call(svc::START, 4, 0)?;
        self.call(svc::START, 5, 0)?;
        self.state(4, svc::FAILED)?;
        let exhausted = self.state(5, svc::FAILED)?;
        require(exhausted[4] == 3 && exhausted[14] == 3)?;
        log(b"SVC-P3 delai de disponibilite et boucle de fautes bornes");
        self.call(svc::START, 6, 0)?;
        self.state(6, svc::RUNNING)?;
        self.call(svc::STOP, 6, 0)?;
        self.state(6, svc::STOPPED)?;
        log(b"SVC-P3 arret force apres delai");
        self.call(svc::RESTART, 9, 0)?;
        self.until(9, |r| r[3] == svc::RUNNING && r[4] == 3 && r[14] >= 2)?;
        require(self.call(svc::QUERY, 9, 0)?[10] == svc::NATIVE_INFO)?;
        Ok(())
    }
}

#[path = "service_caps_race.rs"]
mod caps_race;

/// Profil de qualification 88 uniquement, séquence sur un volume P9 neuf.
fn config_race(fs: &mut fileapi::Client) -> rt::Result<()> {
    let mut admin = Control { tx: rt::config(16), rx: rt::config(17), tag: 0 };
    let mut read = Control { tx: rt::config(11), rx: rt::config(12), tag: 0 };
    read.state(9, svc::RUNNING)?;
    let initial = read.call(svc::SETTINGS, 9, 0)?;
    if initial[12] == 12 {
        return caps_race::run(fs, admin, read);
    }
    require(initial[15] == rt::OK && initial[10] == 1 && initial[12] == 0)?;
    // Deux publications en vol avec la même précondition ; aucune collecte
    // d'acquittement entre les deux dépôts dans la file IPC du gestionnaire.
    for (tag, op) in [(1,svc::DISABLE),(2,svc::ENABLE)] {
        rt::send_frame(admin.tx,tag,&svc::encode(&svc::request(op,9,0)),0,false)?;
    }
    for (tag,status) in [(1,rt::OK),(2,protocol::BUSY)] {
        let msg = rt::receive_for(admin.rx, 15_000)?;
        require(msg.kind == 1 && msg.tag == tag && msg.len == 128)?;
        let f = svc::decode(&msg.payload).ok_or(rt::BAD_ARGUMENT)?;
        require(f[0] == svc::VERSION && f[1] == status && f[2] == 9)?;
    }
    let after = read.call(svc::SETTINGS, 9, 0)?;
    require(after[10] == 0 && after[11] == 1 && after[12] == 1 && after[15] == rt::OK)?;
    log(b"SVC-CFG deux publications CAS 0 en vol : une confirmee, une BUSY, generation 1 relue");

    // Le rendez-vous doit avoir remis l'operation au gestionnaire. La perte
    // de la seule preuve de fin est UNCERTAIN pour ce client.
    admin.tag = 2;
    admin.submit(svc::ENABLE,9,1)?;
    let lost = matches!(rt::receive_for(admin.rx,15_000),
        Err(rt::REVOKED | rt::CLOSED | rt::DENIED));
    require(lost)?;
    log(b"SVC-CFG resultat client UNCERTAIN apres admission et revocation");
    for _ in 0..2 {
        let current = read.call(svc::SETTINGS,9,0)?;
        require(current[10] == 1 && current[12] == 2 && current[15] == rt::OK)?;
    }
    // Vérifier le retrait sans réémettre la mutation dont la réponse manquait.
    require(matches!(admin.submit(svc::SETTINGS,9,0),
        Err(rt::REVOKED | rt::CLOSED | rt::DENIED)))?;
    log(b"SVC-CFG deux relectures P9 generation 2, handle retire, aucune repetition");
    let mut ops = Control {tx: rt::config(14), rx: rt::config(15), tag:0};
    ops.call(svc::SHUTDOWN,0,0)?;
    rt::value(rt::call(instances::SERVICE_PROCESS, service_process::DRAIN, 0, 0))?;
    fs.close(0)?;
    Ok(())
}

fn run_suite() -> rt::Result<()> {
    let mut fs = fileapi::Client::new()?;
    fs.mount()?;
    if rt::config(13) == 88 { return config_race(&mut fs); }
    require(rt::call(instances::SERVICE_PROCESS, service_process::NOW, 0, 0).status == rt::DENIED)?;
    let mut client = Control { tx: rt::config(14), rx: rt::config(15), tag: 0 };
    require(client.tx != 0 && client.rx != 0)?;
    let mut reader = Control { tx: rt::config(11), rx: rt::config(12), tag: 0 };
    require(reader.call(svc::START, 9, 0).err() == Some(rt::DENIED))?;
    require(reader.call(svc::INJECT, 0, 5).err() == Some(rt::DENIED))?;
    client.state(1, svc::RUNNING)?;
    client.state(2, svc::RUNNING)?;
    client.state(9, svc::RUNNING)?;
    client.boot_order()?;
    let utc = client.call(svc::QUERY, 1, 0)?[10];
    require(utc > 0 && client.call(svc::QUERY, 2, 0)?[10] == 0)?;
    log(b"SVC-P3 services actifs P9");
    if rt::config(13) == 1 {
        client.submit(svc::INJECT, 0, 5)?;
        match rt::receive_for(client.rx, 1000) {
            Err(rt::CLOSED | rt::DENIED | rt::REVOKED) => (),
            _ => return Err(rt::REFUSED),
        }
        log(b"SVC-P3 perte gestionnaire canal revoque");
    } else {
        client.storage()?;
        client.stale_ready()?;
        client.failures()?;
        client.call(svc::SHUTDOWN, 0, 0)?;
        log(b"SVC-P3 fermeture volontaire confirmee");
    }
    rt::value(rt::call(instances::SERVICE_PROCESS, service_process::DRAIN, 0, 0))?;
    fs.close(0)?;
    Ok(())
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(run_suite().err().unwrap_or(0))
}
