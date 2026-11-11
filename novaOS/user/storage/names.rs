//! Contrôles par le client natif, contre le service monté dans un autre processus.
use super::*;

fn require_error<T>(outcome: rt::Result<T>, code: u64) -> rt::Result<()> {
    ensure(outcome.err() == Some(code))
}
fn directory_close(c: &mut Client, id: u64) -> rt::Result<()> {
    c.simple(w::DIR_CLOSE, id).map(|_| ())
}
fn body(q: &w::Frame) -> &[u8] {
    &q[w::PAYLOAD..w::PAYLOAD + w::length(q)]
}
fn exact(c: &mut Client, id: u64, value: &[u8]) -> rt::Result<()> {
    let mut data = [0xa5; 16];
    let n = c.read(id, 0, &mut data)?;
    ensure(n == value.len() && &data[..n] == value && data[n] == 0xa5)
}
fn tree(c: &mut Client, root: u64, first: &[u8], rights: u8) -> rt::Result<u64> {
    let mut cur = c.directory(root, first, rights)?;
    for b in b"bcd" {
        let next = c.directory(cur, &[*b; 250], rights)?;
        directory_close(c, cur)?;
        cur = next;
    }
    Ok(cur)
}
pub fn probe(mut c: Client, mode: u64) -> rt::Result<()> {
    c.mount()?;
    let rw = !matches!(mode, 30 | 37 | 38) && rt::config(2) == 0;
    let rights = if rw { w::DIR_ALL } else { w::DIR_RO };
    let root = c.directory(0, b"/", rights)?;
    if mode != 36 {
        require_error(
            c.open(b"/public/marque/enfant", w::READABLE),
            rt::BAD_ARGUMENT,
        )?;
    }
    if mode == 38 {
        let token = rt::config(3);
        if rt::config(2) == 1 {
            let dir = c.directory(root, b"public", rights)?;
            c.simple(w::CHDIR, dir)?;
            rt::send_for(token, 1, b"fin", 15000)?;
            rt::exit(rt::CANCELLED);
        }
        let msg = rt::receive_for(token, 15000)?;
        ensure(msg.kind == 1 && msg.len == 3 && &msg.payload[..3] == b"fin")?;
        let file = c.open_at(root, b"public/marque", 1)?;
        exact(&mut c, file, b"public")?;
        c.close(file)?;
        log(b"NOMS survivant apres depart du pair");
    } else if rt::config(2) == 1 {
        let pubdir = c.directory(root, b"public", rights)?;
        c.simple(w::CHDIR, pubdir)?;
        ensure(body(&c.cwd_fragment(0, 0)?) == b"/public")?;
        require_error(c.at(w::MKDIR_AT, pubdir, b"intrus", 0), rt::DENIED)?;
        require_error(c.directory(pubdir, b"..", rights), rt::DENIED)?;
        require_error(c.directory(pubdir, b"/", rights), rt::DENIED)?;
        let ro = c.open_at(0, b"marque", w::READABLE)?;
        exact(&mut c, ro, b"public")?;
        let channel = rt::config(3);
        let msg = rt::receive_for(channel, 15000)?;
        ensure(msg.kind == 1 && msg.len == 8 && msg.tag == 1)?;
        let foreign = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
        require_error(c.simple(w::STAT, foreign), rt::REVOKED)?;
        ensure(body(&c.cwd_fragment(0, 0)?) == b"/public")?;
        rt::send_for(channel, 2, &ro.to_le_bytes(), 15000)?;
        c.close(ro)?;
        directory_close(&mut c, pubdir)?;
        log(b"NOMS seconde session RO cwd=/public");
    } else if mode == 36 {
        depth(&mut c, root)?;
    } else if mode == 37 {
        let head = if c.at(w::LOOKUP_AT, root, b"a", 0).is_ok() {
            &b"a"[..]
        } else {
            &[b'z'; 255][..]
        };
        let dir = tree(&mut c, root, head, w::DIR_RO)?;
        let f = c.open_at(dir, &[b'e'; 64], 1)?;
        exact(&mut c, f, b"Nova")?;
        c.close(f)?;
        directory_close(&mut c, dir)?;
        log(if head.len() == 1 {
            b"NOMS oracle ancien"
        } else {
            b"NOMS oracle nouveau"
        });
    } else if mode >= 31 {
        c.rename(root, b"a", root, &[b'z'; 255])?;
        log(b"NOMS renommage confirme");
    } else {
        ensure(body(&c.cwd_fragment(0, 0)?) == b"/")?;
        let old = b"a";
        let long = [b'z'; 255];
        let name = [b'e'; 64];
        let parent = tree(&mut c, root, if rw { old } else { &long }, rights)?;
        let file = c.open_at(parent, &name, w::READABLE)?;
        let oid = w::word(&c.simple(w::STAT, file)?, w::PAYLOAD);
        exact(&mut c, file, b"Nova")?;
        c.simple(w::CHDIR, parent)?;
        let channel = rt::config(3);
        rt::send_for(channel, 1, &parent.to_le_bytes(), 15000)?;
        let msg = rt::receive_for(channel, 15000)?;
        ensure(msg.len == 8 && msg.kind == 1 && msg.tag == 2)?;
        require_error(
            c.simple(
                w::STAT,
                u64::from_le_bytes(msg.payload[..8].try_into().unwrap()),
            ),
            rt::REVOKED,
        )?;
        if rw {
            log(b"NOMS avant rename");
            c.rename(root, old, root, &long)?;
            exact(&mut c, file, b"Nova")?;
            let new = c.at(w::LOOKUP_AT, parent, &name, 0)?;
            ensure(oid == w::word(&new, w::PAYLOAD))?;
            let mut pos = 0;
            let mut generation = 0;
            // Nom canonique du répertoire après renommage : quatre composants.
            let mut expected = [0u8; 1009];
            let mut count = 0;
            for (ch, n) in [(b'z', 255), (b'b', 250), (b'c', 250), (b'd', 250)] {
                expected[count] = b'/';
                count += 1;
                expected[count..count + n].fill(ch);
                count += n;
            }
            while pos < count {
                let part = c.cwd_fragment(pos as u64, generation)?;
                ensure(w::word(&part, 32) == count as u64)?;
                generation = w::word(&part, 24);
                let data = body(&part);
                ensure(!data.is_empty() && expected[pos..pos + data.len()] == *data)?;
                pos += data.len();
            }
            let e = c.simple(w::READDIR, parent)?;
            ensure(body(&e) == name)?;
            c.simple(w::CHDIR, root)?;
            c.rename(root, &long, root, old)?;
            c.rename(root, old, root, &long)?;
            c.at(w::MKDIR_AT, root, b"nouveau", 0)?;
            let dir = c.directory(root, b"nouveau", rights)?;
            c.at(w::MKDIR_AT, dir, b"sous", 0)?;
            require_error(c.at(w::MKDIR_AT, dir, b"sous", 0), w::EXISTS)?;
            require_error(c.rename(root, b"nouveau", dir, b"cycle"), rt::BAD_ARGUMENT)?;
            require_error(c.rename(root, b"nouveau", root, b"public"), w::EXISTS)?;
            let nested = c.directory(dir, b"./sous", rights)?;
            let f = c.open_at(nested, b"f", 7)?;
            c.write(f, 0, b"octets")?;
            c.sync(f)?;
            c.rename(nested, b"f", dir, b"g")?;
            exact(&mut c, f, b"octets")?;
            require_error(c.at(w::UNLINK_AT, dir, b"g", 0), w::BUSY)?;
            c.close(f)?;
            c.at(w::UNLINK_AT, dir, b"g", 0)?;
            let limited = c.directory(dir, b".", w::TRAVERSE)?;
            require_error(c.simple(w::READDIR, limited), rt::DENIED)?;
            require_error(c.directory(limited, b"sous", rights), rt::DENIED)?;
            directory_close(&mut c, limited)?;
            c.simple(w::CHDIR, nested)?;
            directory_close(&mut c, nested)?;
            require_error(c.at(w::RMDIR, dir, b"sous", 0), w::BUSY)?;
            c.simple(w::CHDIR, root)?;
            directory_close(&mut c, dir)?;
            require_error(c.at(w::RMDIR, root, b"nouveau", 0), w::NOT_EMPTY)?;
            c.at(w::RMDIR, root, b"nouveau/sous", 0)?;
            c.at(w::RMDIR, root, b"nouveau", 0)?;
            require_error(c.simple(w::STAT, dir), rt::REVOKED)?;
            require_error(c.at(w::LOOKUP_AT, root, b"x//y", 0), rt::BAD_ARGUMENT)?;
            require_error(c.at(w::LOOKUP_AT, root, &[0xff], 0), rt::BAD_ARGUMENT)?;
            log(b"NOMS mutations et refus OK");
        }
        ensure(w::word(&c.simple(w::STAT, file)?, w::PAYLOAD) == oid)?;
        c.close(file)?;
        directory_close(&mut c, parent)?;
        log(b"NOMS lecture par identite OK");
    }
    directory_close(&mut c, root)?;
    c.close(0)?;
    let stale = c.simple(w::INFO, 0).err();
    ensure(matches!(stale, Some(rt::DENIED | rt::CLOSED | rt::REVOKED)))?;
    Ok(())
}

fn depth(c: &mut Client, root: u64) -> rt::Result<()> {
    let x = [b'x'; 255];
    let z = [b'z'; 255];
    let y = [b'y'; 254];
    let v = [b'v'; 254];
    let first = c.readdir_fragment(root, b"", 0, 0)?;
    let generation = w::word(&first, 24);
    ensure(w::word(&first, 32) == 255 && body(&first) == &x[..w::FRAGMENT])?;
    let middle = c.readdir_fragment(root, b"", 88, generation)?;
    let end = c.readdir_fragment(root, b"", 176, generation)?;
    ensure(body(&middle) == &x[88..176] && body(&end) == &x[176..])?;
    ensure(w::length(&c.readdir_fragment(root, &x, 0, generation)?) == 0)?;
    let mut parent = root;
    // Résoudre la demande de 10 001 octets composant par composant. L'image
    // contient le préfixe maximal et le premier enfant manquant est au niveau 33.
    let mut traversed = 0;
    let mut bytes_left = 10001usize;
    while bytes_left > 0 {
        let edge = bytes_left.min(256);
        match c.directory(parent, &x[..edge - 1], w::DIR_RO) {
            Ok(child) => {
                if parent != root {
                    directory_close(c, parent)?;
                }
                parent = child;
                bytes_left -= edge;
                traversed += 1;
            }
            Err(w::NOT_FOUND) => break,
            other => {
                other?;
            }
        }
    }
    ensure(traversed == 32 && bytes_left == 1809)?;
    directory_close(c, parent)?;
    parent = root;
    log(b"ech debut");
    for _ in 1..=32 {
        let h = c.directory(parent, &x, w::DIR_ALL)?;
        if parent != root {
            directory_close(c, parent)?;
        }
        parent = h;
    }
    log(b"ech parcours32");
    c.simple(w::CHDIR, parent)?;
    let first = c.cwd_fragment(0, 0)?;
    ensure(w::word(&first, 32) == 8192 && body(&first)[0] == b'/')?;
    let last = c.cwd_fragment(8190, w::word(&first, 24))?;
    ensure(body(&last) == b"xx")?;
    let generation = w::word(&c.info()?, w::PAYLOAD + 72);
    // Le 33e composant est aussi le premier composant non représentable
    // d'une demande cumulée de 10 001 octets (39 arêtes de 256 puis 17).
    require_error(c.at(w::MKDIR_AT, parent, &x, 0), rt::LIMIT)?;
    require_error(c.at(w::LOOKUP_AT, parent, &x, 0), w::NOT_FOUND)?;
    ensure(w::word(&c.info()?, w::PAYLOAD + 72) == generation)?;
    directory_close(c, parent)?;
    c.simple(w::CHDIR, root)?;
    let mut parent = root;
    for level in 1..32 {
        let child = c.directory(parent, &x, w::DIR_ALL)?;
        if parent != root {
            directory_close(c, parent)?;
        }
        parent = child;
        if matches!(level, 3 | 15 | 31) {
            let files = [&y[..], &z[..]];
            for name in &files[usize::from(level != 3)..] {
                let h = c.open_at(parent, name, w::READABLE)?;
                exact(c, h, b"Nova")?;
                c.close(h)?;
            }
            if level == 3 {
                let d = c.directory(parent, &v, w::DIR_RO)?;
                let f = c.open_at(d, b"k", 1)?;
                exact(c, f, b"Nova")?;
                c.close(f)?;
                directory_close(c, d)?;
            }
        }
    }
    directory_close(c, parent)?;
    // Le même fichier est atteignable dans une requête relative de 1023
    // octets ou dans une requête absolue de 1024.
    let mut path = [b'x'; 1024];
    for pos in [0, 256, 512, 768] {
        path[pos] = b'/';
    }
    path[769..].fill(b'z');
    let one = c.at(w::LOOKUP_AT, root, &path[1..], 0)?;
    let two = c.at(w::LOOKUP_AT, 0, &path, 0)?;
    ensure(body(&one) == body(&two))?;
    let mut bad = w::frame(w::PATH_FRAGMENT, c.session, 0);
    w::size(&mut bad, 1);
    w::set(&mut bad, 32, 1025);
    require_error(c.request(bad), rt::LIMIT)?;
    log(b"ech debut");
    c.at(w::MKDIR_AT, root, b"mesure", 0)?;
    log(b"ech mkdir");
    log(b"ech debut");
    let h = c.open_at(root, b"mesure/test", 7)?;
    log(b"ech open_at");
    log(b"ech debut");
    c.at(w::LOOKUP_AT, root, b"mesure/test", 0)?;
    log(b"ech lookup_at");
    let d = c.directory(root, b"mesure", w::DIR_ALL)?;
    log(b"ech debut");
    c.simple(w::READDIR, d)?;
    log(b"ech readdir");
    c.close(h)?;
    log(b"ech debut");
    c.rename(d, b"test", d, b"final")?;
    log(b"ech rename");
    log(b"ech debut");
    c.at(w::UNLINK_AT, d, b"final", 0)?;
    log(b"ech unlink");
    directory_close(c, d)?;
    c.at(w::RMDIR, root, b"mesure", 0)?;
    log(b"NOMS profondeurs et longueurs OK");
    Ok(())
}
