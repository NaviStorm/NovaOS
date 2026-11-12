//! Sonde expérimentale du vecteur public PC1. Aucune adresse fournie par l'hôte.
use super::*;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

const N: usize = 256;
const BLOC: usize = 4096;
const TAG: usize = BLOC - 32;
type H = Hmac<Sha256>;

fn mac(cle: &[u8], parties: &[&[u8]]) -> H {
    let mut m = H::new_from_slice(cle).unwrap();
    for partie in parties {
        m.update(partie);
    }
    m
}

fn cle(intention: bool) -> [u8; 32] {
    let mut sel = [1; 48];
    sel[16..32].fill(2);
    sel[32..].fill(3);
    let maitresse: [u8; 32] = core::array::from_fn(|i| i as u8);
    let prk = mac(&sel, &[&maitresse]).finalize().into_bytes();
    let info: &[u8] = if intention {
        b"NovaFS/PC1/intent-key\0"
    } else {
        b"NovaFS/PC1/decision-key\0"
    };
    mac(&prk, &[info, &[4; 16], &[1]])
        .finalize()
        .into_bytes()
        .into()
}

struct Rang {
    g: usize,
    racine: Vec<u8>,
    corps: [u8; 208],
}

impl Rang {
    fn nouveau(g: usize, parent: [u8; 32]) -> Self {
        let le = |n: usize| (n as u64).to_le_bytes();
        let mut racine = vec![0; BLOC];
        racine[..8].copy_from_slice(b"NVTST001");
        racine[8..16].copy_from_slice(&le(g));
        let mut c = [0; 208];
        c[..8].copy_from_slice(b"NVPUBC01");
        c[8] = 1;
        c[10] = 1;
        for i in 1..=4 {
            c[i * 16..(i + 1) * 16].fill(i as u8);
        }
        c[80..88].copy_from_slice(b"NVTST001");
        c[96..104].copy_from_slice(&le(N));
        c[104..112].copy_from_slice(&le(g));
        c[112..128].fill((5 + g) as u8);
        c[128..160].copy_from_slice(&parent);
        c[160..168].copy_from_slice(&le(7 + g));
        c[168..176].copy_from_slice(&le(g));
        let empreinte = Sha256::new()
            .chain_update(b"NovaFS/PC1/root\0")
            .chain_update(&c[16..64])
            .chain_update(&c[80..96])
            .chain_update(&c[160..176])
            .chain_update(&racine)
            .finalize();
        c[176..].copy_from_slice(&empreinte);
        Self {
            g,
            racine,
            corps: c,
        }
    }

    fn id(&self) -> [u8; 32] {
        Sha256::new()
            .chain_update(b"NovaFS/PC1/id\0")
            .chain_update(self.corps)
            .finalize()
            .into()
    }

    fn position(&self, role: usize) -> usize {
        [4, 0, 252][role] + self.g - 1
    }

    fn page(&self, role: usize, cle: &[u8]) -> Vec<u8> {
        let mut p = vec![0; BLOC];
        p[..208].copy_from_slice(&self.corps);
        p[208] = role as u8;
        p[216..224].copy_from_slice(&(self.position(role) as u64).to_le_bytes());
        let tag = mac(cle, &[b"NovaFS/PC1/page\0", &p[..TAG]])
            .finalize()
            .into_bytes();
        p[TAG..].copy_from_slice(&tag);
        p
    }
}

fn halte(choix: u8, etape: u8) {
    if choix == etape {
        hal::log(format_args!("PC1 PAUSE {etape}"));
        attendre(b'G');
    }
}

struct Sonde<'a> {
    t: &'a mut Transport,
    base: u64,
    halte: u8,
}

impl Sonde<'_> {
    fn lire(&mut self, bloc: usize, buf: &mut [u8]) -> Result<()> {
        let lba = self.base + (bloc * BLOC / self.t.size) as u64;
        for (i, secteur) in buf.chunks_exact_mut(self.t.size).enumerate() {
            self.t.read(lba + i as u64, secteur)?;
        }
        Ok(())
    }

    fn ecrire(&mut self, bloc: usize, page: &[u8]) -> Result<()> {
        let lba = self.base + (bloc * BLOC / self.t.size) as u64;
        for (i, secteur) in page.chunks_exact(self.t.size).enumerate() {
            self.t.p.0[22].mem.put(0, secteur);
            self.t.output(lba + i as u64, false)?;
            if bloc == 1 && i == 0 {
                halte(self.halte, 10);
            }
        }
        Ok(())
    }

    fn verifier(&mut self, g: &Rang, role: usize, cle: &[u8]) -> Result<()> {
        let mut p = vec![0; BLOC];
        self.lire(g.position(role), &mut p)?;
        mac(cle, &[b"NovaFS/PC1/page\0", &p[..TAG]])
            .verify_slice(&p[TAG..])
            .map_err(|_| Error::Device)?;
        if p != g.page(role, cle) {
            return Err(Error::Device);
        }
        Ok(())
    }

    fn transaction(&mut self, reference: bool, genesis: &Rang) -> Result<()> {
        let g = Rang::nouveau(2, genesis.id());
        let (ki, kd) = (cle(true), cle(false));
        let intention = g.page(0, &ki);
        let freq = hal::timer::counter_hz();
        halte(self.halte, 0);
        let debut = hal::timer::counter();
        self.ecrire(9, &g.racine)?;
        halte(self.halte, 1);
        if !reference {
            self.ecrire(g.position(0), &intention)?;
        }
        halte(self.halte, 2);
        let f1 = hal::timer::counter();
        self.t.output(0, true)?;
        let fin1 = hal::timer::counter();
        halte(self.halte, 3);
        if !reference {
            self.verifier(&g, 0, &ki)?;
            let mut w = vec![0; BLOC];
            self.lire(9, &mut w)?;
            if w != g.racine {
                return Err(Error::Device);
            }
        }
        halte(self.halte, 4);
        self.ecrire(g.position(1), &g.page(1, &kd))?;
        halte(self.halte, 5);
        if !reference {
            self.ecrire(g.position(2), &g.page(2, &kd))?;
        }
        halte(self.halte, 6);
        let f2 = hal::timer::counter();
        self.t.output(0, true)?;
        let fin2 = hal::timer::counter();
        halte(self.halte, 7);
        if !reference {
            self.verifier(&g, 1, &kd)?;
            self.verifier(&g, 2, &kd)?;
        }
        halte(self.halte, 8);
        let duree = hal::timer::counter() - debut;
        let id = Sha256::new()
            .chain_update(b"NovaFS/PC1/id\0")
            .chain_update(g.corps)
            .finalize();
        hal::log(format_args!(
            "PC1 {} g=2 id={id:x} ticks={duree} hz={freq} f1={} f2={}",
            if reference { "TEMOIN" } else { "ACK" },
            fin1 - f1,
            fin2 - f2
        ));
        halte(self.halte, 9);
        Ok(())
    }
}

pub(super) fn essai(t: &mut Transport, args: &[u8]) -> Result<()> {
    // Seul le couple série/SID de qualification est ouvert par l'appelant.
    let ascii = args.strip_suffix(&[0]).unwrap_or(args);
    if ascii.len() != 64 {
        return Err(Error::Geometry);
    }
    let mut v = [0u8; 32];
    for (b, chars) in v.iter_mut().zip(ascii.chunks_exact(2)) {
        *b = u8::from_str_radix(
            core::str::from_utf8(chars).map_err(|_| Error::Geometry)?,
            16,
        )
        .map_err(|_| Error::Geometry)?;
    }
    identite(t)?;
    let (mio, taille) = match t.size {
        512 => (61, 64),
        4096 => (265, 268),
        _ => return Err(Error::Geometry),
    };
    if t.ro
        || t.sectors != taille * MIO / t.size as u64
        || &v[..8] != b"NVPC1Q01"
        || nombre(&v[8..12]) as usize != t.size
        || v[12..28] == [0; 16]
        || v[28] > 2
        || v[30..] != [0; 2]
        || (v[29] > 12 && v[29] != 255)
    {
        return Err(Error::Geometry);
    }
    let mut marque = vec![0; t.size];
    t.read(0, &mut marque)?;
    if marque[..28] != v[..28] || marque[28..].iter().any(|&x| x != 0) {
        return Err(Error::Device);
    }
    let base = mio * MIO / t.size as u64;
    let mut sonde = Sonde {
        t,
        base,
        halte: v[29],
    };
    let mut p = vec![0; BLOC];
    if v[28] == 2 {
        let mut sha = Sha256::new();
        for b in 0..N {
            sonde.lire(b, &mut p)?;
            sha.update(&p);
        }
        hal::log(format_args!("PC1 RELECTURE {:x}", sha.finalize()));
        return Ok(());
    }
    let genesis = Rang::nouveau(1, [0; 32]);
    let initial = [
        genesis.page(0, &cle(true)),
        genesis.page(1, &cle(false)),
        genesis.page(2, &cle(false)),
    ];
    let mut verifier_genese = |sonde: &mut Sonde<'_>| -> Result<()> {
        for b in 0..N {
            sonde.lire(b, &mut p)?;
            let exact = match b {
                4 => p == initial[0],
                0 => p == initial[1],
                252 => p == initial[2],
                8 => p == genesis.racine,
                _ => p.iter().all(|&x| x == 0),
            };
            if !exact {
                return Err(Error::Device);
            }
        }
        Ok(())
    };
    verifier_genese(&mut sonde)?;
    halte(sonde.halte, 11);
    autoriser_sortie(sonde.t)?;
    let debut = hal::timer::counter();
    sonde.t.output(0, true)?;
    let duree = hal::timer::counter() - debut;
    hal::log(format_args!(
        "PC1 ACTIVATION ticks={duree} hz={}",
        hal::timer::counter_hz()
    ));
    halte(sonde.halte, 12);
    verifier_genese(&mut sonde)?;
    hal::log(format_args!("PC1 PRET secteur={}", sonde.t.size));
    sonde.transaction(v[28] == 1, &genesis)
}
