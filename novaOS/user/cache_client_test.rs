//! Banc natif du cache. Le lanceur administre des instances isolées via les
//! canaux du gestionnaire. Seuls ces acteurs reçoivent les identités de test.
#![no_std]
#![no_main]
#[allow(dead_code)]
#[path = "../abi/cache.rs"]
mod cache;
#[allow(dead_code)]
#[path = "storage/fileapi.rs"]
mod fileapi;
#[allow(dead_code)]
#[path = "../abi/instances.rs"]
mod instances;
#[path = "services/protocol.rs"]
mod p;
#[allow(dead_code)]
#[path = "../abi/service_process.rs"]
mod process;
#[allow(dead_code)]
#[path = "../abi/files.rs"]
mod protocol;
#[path = "storage/runtime.rs"]
mod runtime;
#[path = "services/cache_steps.rs"]
mod step;
use runtime as rt;

// Les noms ne véhiculent aucune autorité. Ces identifiants sélectionnent le
// manifeste compilé du banc, sans fournir de clé de propriétaire aux processus.
const FIRST: u64 = 11;
const SECOND: u64 = 12;
const OTHER_APP: u64 = 13;
const OTHER_USER: u64 = 14;
const ADMIN: u64 = 15;

fn log(line: &[u8]) {
    rt::call(rt::DEBUG_WRITE, line.as_ptr() as u64, line.len() as u64, 0);
}

fn number(mut n: u64, bytes: &mut [u8; 20]) -> &[u8] {
    let mut i = bytes.len();
    loop {
        i -= 1;
        bytes[i] = (n % 10) as u8 + b'0';
        n /= 10;
        if n == 0 {
            return &bytes[i..];
        }
    }
}

fn diagnostic(label: &[u8], values: &[u64]) {
    let mut line = [0; 128];
    line[..label.len()].copy_from_slice(label);
    let mut pos = label.len();
    for &n in values {
        let mut digits = [0; 20];
        let field = number(n, &mut digits);
        line[pos] = b' ';
        pos += 1;
        line[pos..pos + field.len()].copy_from_slice(field);
        pos += field.len();
    }
    log(&line[..pos]);
}

#[track_caller]
fn require(ok: bool) -> rt::Result<()> {
    if ok {
        return Ok(());
    }
    diagnostic(
        b"CACHE-P4 assertion ligne",
        &[core::panic::Location::caller().line().into()],
    );
    Err(rt::REFUSED)
}

struct Lab {
    tx: u64,
    rx: u64,
    tag: u64,
}

impl Lab {
    fn call(&mut self, op: u64, id: u64, arg: u64, value: u64) -> rt::Result<p::Frame> {
        self.tag = self.tag.checked_add(1).ok_or(rt::LIMIT)?;
        let mut q = p::request(op, id, arg);
        q[4] = value;
        rt::send_for(self.tx, self.tag, &p::encode(&q), 15_000)?;
        let reply = rt::receive_for(self.rx, 15_000)?;
        require(reply.kind == 1 && reply.tag == self.tag && reply.len == 128)?;
        let f = p::decode(&reply.payload).ok_or(rt::BAD_ARGUMENT)?;
        require(f[0] == p::VERSION && f[2] == id)?;
        if f[1] != rt::OK {
            return Err(f[1]);
        }
        Ok(f)
    }

    #[track_caller]
    fn action(&mut self, id: u64, step: u64, arg: u64, expected: u64) -> rt::Result<u64> {
        match self.call(p::CACHE_TEST, id, step, arg) {
            Ok(f) if expected == rt::OK => Ok(f[10]),
            Err(e) if e == expected => Ok(0),
            outcome => {
                diagnostic(
                    b"CACHE-P4 erreur acteur action attendu obtenu ligne",
                    &[
                        id,
                        step,
                        expected,
                        outcome.err().unwrap_or(rt::OK),
                        core::panic::Location::caller().line().into(),
                    ],
                );
                Err(rt::REFUSED)
            }
        }
    }

    fn state(&mut self, id: u64, desired: u64) -> rt::Result<p::Frame> {
        for _ in 0..5000 {
            let f = self.call(p::STATUS, id, 0, 0)?;
            if f[3] == desired {
                return Ok(f);
            }
            if matches!(f[3], p::STOPPED | p::FAILED) && desired == p::RUNNING {
                diagnostic(
                    b"CACHE-P4 service indisponible id etat tentative",
                    &[id, f[3], f[4]],
                );
                return Err(rt::REFUSED);
            }
            match rt::receive_for(self.rx, 2) {
                Err(rt::TIMED_OUT) => (),
                Err(e) => return Err(e),
                Ok(_) => return Err(rt::REFUSED),
            }
        }
        Err(rt::TIMED_OUT)
    }

    fn start(&mut self, id: u64) -> rt::Result<p::Frame> {
        self.call(p::START, id, 0, 0)?;
        self.state(id, p::RUNNING)
    }

    fn stop(&mut self, id: u64) -> rt::Result<()> {
        self.call(p::STOP, id, 0, 0)?;
        self.state(id, p::STOPPED).map(|_| ())
    }

    fn admit(&mut self, id: u64) -> rt::Result<()> {
        self.call(p::REGISTER, id, 0, 0)?;
        self.start(id).map(|_| ())
    }

    fn opened(&mut self, id: u64) -> rt::Result<u64> {
        self.action(id, step::OPEN, 0, rt::OK)
    }

    fn check(&mut self, id: u64, byte: u64) -> rt::Result<()> {
        require(self.action(id, step::READ, 0, rt::OK)? == byte)
    }

    fn copy_token(&mut self, from: u64, to: u64) -> rt::Result<()> {
        for i in 0..4 {
            let word = self.action(from, step::EXPORT + i, 0, rt::OK)?;
            self.action(to, step::IMPORT + i, word, rt::OK)?;
        }
        Ok(())
    }

    fn race(&mut self) -> rt::Result<()> {
        self.call(p::CACHE_ASYNC_WRITE, SECOND, step::RACE_WRITE, 153)?;
        let mut sent = false;
        for _ in 0..1000 {
            match self.call(p::CACHE_TEST, FIRST, step::RACE_SENT, 0) {
                Ok(_) => {
                    sent = true;
                    break;
                }
                Err(rt::WOULD_BLOCK) => (),
                Err(e) => return Err(e),
            }
        }
        require(sent)?;
        self.action(FIRST, step::PURGE, 0, rt::OK)?;
        self.action(SECOND, step::RACE_RESULT, 0, rt::OK)?;
        self.action(SECOND, step::WRITE, 127, rt::REVOKED)?;
        self.action(FIRST, step::READ, 0, rt::REVOKED)?;
        self.opened(FIRST)?;
        self.action(FIRST, step::RACE_READ, 0, protocol::NOT_FOUND)?;
        self.action(FIRST, step::READ, 0, protocol::NOT_FOUND)?;
        log(b"CACHE-P4 ecriture remise purge concurrente anciens droits retires");
        Ok(())
    }

    // Le lanceur tient les racines comme nsh. NovaFS reçoit exclusivement la
    // réception des demandes et l'envoi des réponses. Le témoin exerce les deux
    // canaux avant et après l'E/S du processus isolé pendant le même prêt.
    fn delegated_storage(&mut self, fs: &mut fileapi::Client) -> rt::Result<()> {
        struct Endpoints([u64; 2]);
        impl Drop for Endpoints {
            fn drop(&mut self) {
                for &h in &self.0 {
                    if h != 0 { rt::call(rt::IPC_CLOSE, h, 0, 0); }
                }
            }
        }

        let file = fs.open_at(0, b"/documents/exemple.txt", protocol::READABLE)?;
        let result = (|| {
            let mut ports = Endpoints([0; 2]);
            for h in &mut ports.0 {
                *h = rt::value(rt::call(rt::IPC_CREATE, 0, 0, 0))?;
            }
            let [requests, replies] = ports.0;
            fs.give(requests, rt::RECEIVE)?;
            fs.give(replies, rt::SEND)?;
            fs.delegate(protocol::FILE_VIEW, file, &[])?;
            let mut loan = fileapi::Client::scoped_ports((requests, replies));
            let mut bytes = [0; 7];
            require(loan.read(1, 0, &mut bytes)? == 7 && &bytes == b"NovaOS\n")?;
            self.action(FIRST, step::WRITE, 41, rt::OK)?;
            self.check(FIRST, 41)?;
            self.action(FIRST, step::TMP, 0, rt::OK)?;
            bytes.fill(0);
            require(loan.read(1, 0, &mut bytes)? == 7 && &bytes == b"NovaOS\n")
        })();
        // La fin de portée retire les deux racines. Le retour du canal principal
        // prouve aussi que la boucle de délégation a rendu la session.
        let closed = fs.close(file);
        result.and(closed)?;
        self.check(FIRST, 41)?;
        log(b"CACHE-P4 pret VFS lecture cache ecriture lecture tmp retrait OK");
        Ok(())
    }

    fn functional(&mut self, fs: &mut fileapi::Client) -> rt::Result<()> {
        self.admit(FIRST)?;
        self.admit(SECOND)?;
        let a = self.opened(FIRST)?;
        require(self.opened(SECOND)? == a)?;
        self.delegated_storage(fs)?;
        self.action(FIRST, step::WRITE, 42, rt::OK)?;
        self.check(SECOND, 42)?;
        self.copy_token(FIRST, SECOND)?;
        self.action(SECOND, step::FOREIGN_READ, 0, rt::REVOKED)?;
        self.action(SECOND, step::FORGE, 0, rt::OK)?;
        self.action(SECOND, step::NAMES, 0, rt::OK)?;
        self.action(SECOND, step::PURGE_USER, 0, rt::DENIED)?;
        self.action(SECOND, step::PURGE_SYSTEM, 0, rt::DENIED)?;
        self.action(SECOND, step::QUOTA, 0, protocol::FULL)?;
        self.check(SECOND, 42)?;
        self.stop(FIRST)?;
        self.check(SECOND, 42)?;
        self.start(FIRST)?;
        self.opened(FIRST)?;
        self.check(FIRST, 42)?;
        // Les deux incarnations exercent leurs droits sur le même objet.
        self.race()?;
        self.action(FIRST, step::WRITE, 42, rt::OK)?;
        self.stop(SECOND)?;

        self.admit(OTHER_APP)?;
        self.opened(OTHER_APP)?;
        self.action(OTHER_APP, step::READ, 0, protocol::NOT_FOUND)?;
        self.copy_token(FIRST, OTHER_APP)?;
        self.action(OTHER_APP, step::FOREIGN_WRITE, 22, rt::REVOKED)?;
        self.action(OTHER_APP, step::WRITE, 73, rt::OK)?;
        self.admit(OTHER_USER)?;
        self.opened(OTHER_USER)?;
        self.action(OTHER_USER, step::READ, 0, protocol::NOT_FOUND)?;
        self.copy_token(FIRST, OTHER_USER)?;
        self.action(OTHER_USER, step::FOREIGN_READ, 0, rt::REVOKED)?;
        self.action(OTHER_USER, step::WRITE, 91, rt::OK)?;
        self.action(FIRST, step::PURGE, 0, rt::OK)?;
        self.check(OTHER_APP, 73)?;
        self.check(OTHER_USER, 91)?;
        log(b"CACHE-P4 partage isolation usurpation chemins quota et purge ciblee OK");

        // Libère le nombre d'endpoints du premier acteur avant l'admission admin.
        self.stop(FIRST)?;
        self.admit(ADMIN)?;
        self.opened(ADMIN)?;
        self.action(ADMIN, step::PURGE_USER, 0, rt::OK)?;
        self.action(OTHER_APP, step::WRITE, 1, rt::REVOKED)?;
        self.check(OTHER_USER, 91)?;
        self.opened(OTHER_APP)?;
        self.action(OTHER_APP, step::READ, 0, protocol::NOT_FOUND)?;
        self.opened(ADMIN)?;
        self.action(ADMIN, step::PURGE_SYSTEM, 0, rt::OK)?;
        self.action(OTHER_USER, step::READ, 0, rt::REVOKED)?;
        self.opened(OTHER_USER)?;
        self.action(OTHER_USER, step::READ, 0, protocol::NOT_FOUND)?;
        self.stop(ADMIN)?;
        self.opened(OTHER_APP)?;
        self.action(OTHER_APP, step::WRITE, 73, rt::OK)?;
        self.action(OTHER_USER, step::WRITE, 91, rt::OK)?;
        self.stop(OTHER_USER)?;
        self.start(FIRST)?;
        self.opened(FIRST)?;
        self.action(FIRST, step::WRITE, 42, rt::OK)?;
        let previous = self.state(FIRST, p::RUNNING)?;
        require(matches!(
            self.call(p::CACHE_TEST, FIRST, step::CRASH, 0),
            Err(rt::CLOSED)
        ))?;
        self.state(FIRST, p::FAILED)?;
        let new = self.start(FIRST)?;
        require(previous[6..10] != new[6..10])?;
        self.opened(FIRST)?;
        self.check(FIRST, 42)?;
        let provider = self.state(10, p::RUNNING)?;
        self.stop(10)?;
        match self.call(p::CACHE_TEST, FIRST, step::WRITE, 1) {
            Err(rt::CLOSED | rt::REVOKED) => (),
            _ => return Err(rt::REFUSED),
        }
        let again = self.start(10)?;
        require(provider[6..10] != again[6..10])?;
        self.action(FIRST, step::WRITE, 1, rt::REVOKED)?;
        self.opened(FIRST)?;
        self.check(FIRST, 42)?;
        self.opened(OTHER_APP)?;
        self.check(OTHER_APP, 73)?;
        self.action(OTHER_APP, step::SAVE_TOKEN, 0, rt::OK)?;
        log(b"CACHE-P4 purge utilisateur systeme perte service crash relance OK");
        Ok(())
    }

    fn pressure(&mut self, fs: &mut fileapi::Client) -> rt::Result<()> {
        for (id, byte) in [(FIRST, 42), (OTHER_APP, 73), (OTHER_USER, 91)] {
            self.admit(id)?;
            self.opened(id)?;
            self.action(id, step::WRITE, byte, rt::OK)?;
            if id == FIRST {
                self.action(FIRST, step::PAD, 0, rt::OK)?;
            }
        }
        let payload = [0x75u8; 4 * 4096];
        let available = |fs: &mut fileapi::Client| {
            Ok::<_, u64>(protocol::word(&fs.info()?, protocol::PAYLOAD + 48))
        };
        let h = fs.open(
            b"/charge-cache-qualification",
            protocol::CREATE | protocol::WRITABLE | protocol::READABLE,
        )?;
        let stats = fs.info()?;
        let capacity = protocol::word(&stats, protocol::PAYLOAD + 40);
        let start = available(fs)?;
        let mut free = start;
        let mut offset = 0u64;
        require(start >= 128)?;
        while free >= 128 {
            let length = if free > 136 { payload.len() } else { 4096 };
            // Une image de banc consomme au plus sa capacité, même si le seuil
            // n'est jamais franchi à la suite d'un défaut de comptabilité.
            require(offset / 4096 < capacity)?;
            fs.write(h, offset, &payload[..length])?;
            fs.sync(h)?;
            offset += length as u64;
            free = available(fs)?;
            // Le service peut effectuer son entretien périodique juste après SYNC.
            // Une remontée notable des blocs avant INFO déclenche la vérification
            // effective du jeton plutôt que d'écrire jusqu'à la victime suivante.
            if free >= 128 && free < 165 {
                match self.call(p::CACHE_TEST, FIRST, step::READ, 0) {
                    Err(rt::REVOKED) => break,
                    Ok(f) => require(f[10] == 42)?,
                    Err(e) => return Err(e),
                }
            }
        }
        diagnostic(
            b"CACHE-P4 avant entretien libres initiaux courants octets",
            &[start, free, offset],
        );
        require(
            self.call(p::CACHE_MAINTAIN, 10, 0, 0)?[10] == u32::from_le_bytes(*b"CACH") as u64,
        )?;
        self.action(FIRST, step::WRITE, 127, rt::REVOKED)?;
        self.check(OTHER_APP, 73)?;
        self.check(OTHER_USER, 91)?;
        let after = available(fs)?;
        diagnostic(b"CACHE-P4 apres entretien blocs libres", &[after]);
        require(after >= 128)?;
        fs.close(h)?;
        // Réouverture par le client public et vérification des octets après purge.
        let h = fs.open(b"/charge-cache-qualification", protocol::READABLE)?;
        let mut page = [0; 4096];
        for page_no in 0..offset / 4096 {
            let count = fs.read_grouped(h, page_no * 4096, &mut page)?;
            require(count == page.len() && page.iter().all(|&v| v == 0x75))?;
        }
        require(fs.read(h, offset, &mut page[..1])? == 0)?;
        fs.close(h)?;
        Ok(())
    }

    fn corrupt_owner(&mut self) -> rt::Result<()> {
        self.admit(FIRST)?;
        self.action(FIRST, step::OPEN, 0, rt::REFUSED)?;
        // La quarantaine du premier propriétaire laisse lisibles les objets
        // authentifiés des deux autres paires (utilisateur, installation).
        for (id, byte) in [(OTHER_APP, 73), (OTHER_USER, 91)] {
            self.admit(id)?;
            self.opened(id)?;
            self.check(id, byte)?;
        }
        Ok(())
    }

    fn resume(&mut self, profile: u64) -> rt::Result<()> {
        for (id, byte) in [(FIRST, 42), (OTHER_APP, 73), (OTHER_USER, 91)] {
            self.admit(id)?;
            let generation = self.opened(id)?;
            diagnostic(
                b"CACHE-P4 incarnation disque acteur generation",
                &[id, generation],
            );
            if profile == 84 && id == FIRST {
                self.action(FIRST, step::READ, 0, protocol::NOT_FOUND)?;
            } else {
                self.check(id, byte)?;
            }
            if id == OTHER_APP {
                self.action(id, step::LOAD_TOKEN, 0, rt::OK)?;
                self.action(id, step::FOREIGN_WRITE, 1, rt::REVOKED)?;
                self.check(id, byte)?;
            }
            self.stop(id)?;
        }
        Ok(())
    }
}

fn run_suite() -> rt::Result<()> {
    let profile = rt::config(13);
    let mut fs = fileapi::Client::new()?;
    fs.mount()?;
    require(
        rt::call(
            cache::CONTROL,
            cache::OPEN,
            0,
            [0u8; cache::FRAME].as_mut_ptr() as u64,
        )
        .status
            == rt::DENIED,
    )?;
    let mut lab = Lab {
        tx: rt::config(14),
        rx: rt::config(15),
        tag: 0,
    };
    let mut read_only = Lab {
        tx: rt::config(11),
        rx: rt::config(12),
        tag: 0,
    };
    require(read_only.call(p::CACHE_TEST, FIRST, step::OPEN, 0).err() == Some(rt::DENIED))?;
    require(read_only.call(p::CACHE_MAINTAIN, 10, 0, 0).err() == Some(rt::DENIED))?;
    lab.state(10, p::RUNNING)?;
    require(lab.call(p::QUERY, 10, 0, 0)?[10] == u32::from_le_bytes(*b"CACH") as u64)?;
    let success: &[u8] = match profile {
        78 => {
            lab.functional(&mut fs)?;
            b"CACHE-P4 fonctionnel OK" as &[u8]
        }
        79 | 83 | 84 => {
            lab.resume(profile)?;
            match profile {
                79 => b"CACHE-P4 reprise OK",
                83 => b"CACHE-P4 actualisation OK",
                _ => b"CACHE-P4 reprise-purge OK",
            }
        }
        85 => {
            lab.pressure(&mut fs)?;
            b"CACHE-P4 pression OK"
        }
        86 => {
            lab.corrupt_owner()?;
            b"CACHE-P4 proprietaire invalide refuse"
        }
        80 => {
            lab.admit(FIRST)?;
            lab.opened(FIRST)?;
            lab.check(FIRST, 42)?;
            lab.action(FIRST, step::PURGE, 0, rt::OK)?;
            // Le banc hôte doit avoir interrompu QEMU au point de persistance.
            return Err(rt::REFUSED);
        }
        81 | 82 => {
            lab.admit(FIRST)?;
            lab.action(FIRST, step::OPEN, 0, rt::DENIED)?;
            lab.action(FIRST, step::TMP, 0, rt::OK)?;
            if profile == 81 {
                b"CACHE-P4 restauration refusee"
            } else {
                b"CACHE-P4 reinstallation refusee"
            }
        }
        _ => return Err(rt::BAD_ARGUMENT),
    };
    lab.call(p::SHUTDOWN, 0, 0, 0)?;
    rt::value(rt::call(instances::SERVICE_PROCESS, process::DRAIN, 0, 0))?;
    fs.close(0)?;
    log(success);
    Ok(())
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    let status = run_suite().err().unwrap_or(rt::OK);
    if status != rt::OK {
        diagnostic(b"CACHE-P4 echec statut", &[status]);
    }
    rt::exit(status)
}
