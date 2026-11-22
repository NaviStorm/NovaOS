//! Adaptateur hôte de qualification. Aucun secret ni décision d'autorité.
mod cellule;
use cellule::*;
use std::fs::File;
use std::io::{self, ErrorKind, Read, Write};
use std::net::Shutdown;
use std::os::fd::{FromRawFd, RawFd};
use std::os::unix::{fs::FileExt, net::{UnixStream,UnixListener}};

const VOLUME: u64 = 512 * PAGE as u64;
const MAX_IO: usize = 128 * 1024;
mod etat;
use etat::{META, State};

struct Link {
    state: State,
    tx: [u8; PAGE],
    rx: [u8; PAGE],
    relay: UnixStream,
}
const EXTRA: usize = size_of::<Link>() - 2 * PAGE;
const _: () = assert!(META <= 256 && EXTRA <= 256);

impl Link {
    fn new(relay: UnixStream) -> io::Result<Self> {
        relay.set_nonblocking(true)?;
        Ok(Self {
            state: State::default(),
            tx: [0; PAGE],
            rx: [0; PAGE],
            relay,
        })
    }
    fn close(&mut self) {
        let _ = self.relay.shutdown(Shutdown::Both);
        self.tx.fill(0);
        self.rx.fill(0);
        self.state = State {
            closed: true,
            ..State::default()
        };
    }
    fn progress(&mut self) {
        if self.state.closed {
            return;
        }
        if self.state.sent < self.state.length {
            let part = &self.tx
                [HEADER + usize::from(self.state.sent)..HEADER + usize::from(self.state.length)];
            match self.relay.write(part) {
                Ok(0) => {
                    self.close();
                    return;
                }
                Ok(n) => self.state.sent += u16::try_from(n).unwrap(),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => (),
                Err(_) => {
                    self.close();
                    return;
                }
            }
        }
        if self.state.rx == self.state.ack && self.state.tx != 0 {
            match self.relay.read(&mut self.rx[HEADER..HEADER + MAX]) {
                Ok(0) => self.close(),
                Ok(n) => {
                    if let Some(next) = self.state.rx.checked_add(1) {
                        self.state.rx = next;
                        self.state.received = u16::try_from(n).unwrap();
                    } else {
                        self.close()
                    }
                }
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => (),
                Err(_) => self.close(),
            }
        }
    }
    fn emit(&mut self, data: &[u8]) {
        if self.state.closed {
            return;
        }
        let Ok(c) = decode_sector(data) else {
            self.close();
            return;
        };
        let s = &self.state;
        if s.tx != 0 && c.sequence == s.tx && data == &self.tx[..data.len()] {
            self.progress();
            return;
        }
        if s.tx.checked_add(1) != Some(c.sequence)
            || c.link == [0; 16]
            || (s.tx == 0) != (c.kind == OPEN)
            || (s.tx != 0 && c.link != s.link)
            || c.kind == BROKEN
            || !(s.ack..=s.rx).contains(&c.ack)
        {
            self.close();
            return;
        }
        if c.kind == CLOSE {
            self.close();
            return;
        }
        if s.sent < s.length {
            self.progress();
            return;
        }
        if c.ack != s.ack {
            self.rx.fill(0);
            self.state.received = 0;
            self.state.ack = c.ack;
        }
        self.tx[..data.len()].copy_from_slice(data);
        self.state.link = c.link;
        self.state.tx = c.sequence;
        self.state.sent = 0;
        self.state.length = c.size;
        self.progress()
    }
    fn receive(&mut self, sector: usize) -> &[u8] {
        self.progress();
        let s = &self.state;
        let kind = if s.closed {
            BROKEN
        } else if s.received != 0 {
            DATA
        } else {
            EMPTY
        };
        Cell {
            kind,
            size: s.received,
            link: s.link,
            sequence: s.rx,
            ack: s.tx - u64::from(s.sent < s.length),
        }
        .encode(&mut self.rx[..HEADER])
        .expect("état de cellule");
        &self.rx[..sector]
    }
}
impl Drop for Link {
    fn drop(&mut self) {
        self.close()
    }
}

/// Contrôle hôte explicitement sélectionné pour les consultations successives.
/// Ni les octets de clé ni les décisions NovaAuth ne passent par cette structure.
struct Reopener { listener:UnixListener, previous:[u8;16] }
const _: () = assert!(EXTRA + size_of::<Reopener>() <= 256);
impl Reopener {
    fn open(&mut self,link:&mut Link,data:&[u8])->io::Result<()> {
        let Ok(cell)=decode_sector(data) else {return Ok(());};
        if cell.kind!=OPEN || cell.sequence!=1 || cell.ack!=0 ||
            cell.size!=0 || cell.link==[0;16] {return Ok(());}
        if !link.state.closed {
            if link.state.tx==0 {self.previous=cell.link;}
            return Ok(());
        }
        if cell.link==self.previous {return Ok(());}
        // accept non bloquant : une panne du relais ne bloque jamais P9.
        let (stream,_)=match self.listener.accept() {
            Ok(pair)=>pair,
            Err(e) if matches!(e.kind(),ErrorKind::WouldBlock|ErrorKind::Interrupted)=>return Ok(()),
            Err(e)=>return Err(e),
        };
        stream.set_nonblocking(true)?;
        link.relay=stream; // l'ancien socket a déjà subi shutdown, puis est fermé
        link.tx.fill(0);link.rx.fill(0);link.state=State::default();
        self.previous=cell.link;
        Ok(())
    }
}
// Option explicite du banc R2 ; les deux adresses volatiles et les bornes
// restent celles du contrat. La cellule reste limitée à 448 octets utiles.
fn transport_size(n:usize,sector:usize)->bool {
    n==sector || (cfg!(r2_page_512) && sector==512 && n==PAGE)
}
fn serve(disk: File, mut peer: UnixStream, relay: UnixStream, sector: usize,
         mut reopen:Option<Reopener>) -> io::Result<()> {
    let info = disk.metadata()?;
    if !info.is_file() || info.len() != VOLUME || !matches!(sector, 512 | 4096) {
        return Err(ErrorKind::InvalidInput.into());
    }
    let mut link = Link::new(relay)?;
    let mut greeting = [0; 152];
    greeting[..8].copy_from_slice(b"NBDMAGIC");
    greeting[8..16].copy_from_slice(&0x420281861253u64.to_be_bytes());
    greeting[16..24].copy_from_slice(&(VOLUME + 2 * PAGE as u64).to_be_bytes());
    greeting[24..28].copy_from_slice(&5u32.to_be_bytes());
    peer.write_all(&greeting)?;
    // Transit d’une requête bloc, séparé de l’état de liaison. Aucune file de fragments.
    let mut scratch = [0; MAX_IO];
    let mut header = [0; 28];
    loop {
        match peer.read(&mut header[..1])? {
            0 => return Ok(()),
            _ => peer.read_exact(&mut header[1..])?,
        }
        let flags = u16::from_be_bytes(header[4..6].try_into().unwrap());
        let cmd = u16::from_be_bytes(header[6..8].try_into().unwrap());
        let offset = u64::from_be_bytes(header[16..24].try_into().unwrap());
        let n = u32::from_be_bytes(header[24..28].try_into().unwrap()) as usize;
        if header[..4] != 0x25609513u32.to_be_bytes() || (cmd == 1 && n > MAX_IO) {
            return Err(ErrorKind::InvalidData.into());
        }
        if cmd == 2 {
            return Ok(());
        }
        if cmd == 1 {
            peer.read_exact(&mut scratch[..n])?;
        }
        let mut data: &[u8] = &[];
        let status = if flags != 0 || n > MAX_IO {
            22
        } else if cmd == 3 && offset == 0 && n == 0 {
            if disk.sync_all().is_ok() { 0 } else { 5 }
        } else if cmd > 1 || n == 0 || n % sector != 0 || offset % sector as u64 != 0 {
            22
        } else if offset
            .checked_add(n as u64)
            .is_some_and(|end| end <= VOLUME)
        {
            let result = if cmd == 1 {
                disk.write_all_at(&scratch[..n], offset)
            } else {
                disk.read_exact_at(&mut scratch[..n], offset)
            };
            if result.is_err() {
                5
            } else {
                if cmd == 0 {
                    data = &scratch[..n]
                };
                0
            }
        } else if cmd == 1 && offset == VOLUME && transport_size(n,sector) {
            if let Some(r)=reopen.as_mut() {r.open(&mut link,&scratch[..n])?;}
            link.emit(&scratch[..n]);
            0
        } else if cmd == 0 && offset == VOLUME + PAGE as u64 && transport_size(n,sector) {
            data = link.receive(n);
            0
        } else {
            22
        };
        let mut reply = [0; 16];
        reply[..4].copy_from_slice(&0x67446698u32.to_be_bytes());
        reply[4..8].copy_from_slice(&(status as u32).to_be_bytes());
        reply[8..].copy_from_slice(&header[8..16]);
        println!("{cmd} {offset} {n} {flags} {status}");
        peer.write_all(&reply)?;
        peer.write_all(data)?;
    }
}

fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--mesure"] {
        let encoded = State::default().encode();
        assert_eq!(State::decode(&encoded), Ok(State::default()));
        println!(
            "{{\"pages_utiles\":8192,\"etat_natif\":{},\"supplement\":{EXTRA},\"serialise\":{META},\"total_octets\":{},\"plafond\":256}}",
            size_of::<State>(),
            size_of::<Link>()
        );
        println!("{{\"reouverture_optionnelle\":{},\"supplement_avec_reouverture\":{},\"plafond\":256}}",
            size_of::<Reopener>(), EXTRA+size_of::<Reopener>());
        return Ok(());
    }
    if args.len() != 4 && !(args.len()==6 && args[4]=="--reopen-listener") {
        return Err(ErrorKind::InvalidInput.into());
    }
    let fd = |v: &str| {
        v.parse::<RawFd>()
            .ok()
            .filter(|n| *n > 2)
            .ok_or(ErrorKind::InvalidInput)
    };
    let (disk, client, relay) = (fd(&args[0])?, fd(&args[1])?, fd(&args[2])?);
    if disk == client || client == relay || disk == relay {
        return Err(ErrorKind::InvalidInput.into());
    }
    let sector = args[3].parse().map_err(|_| ErrorKind::InvalidInput)?;
    let extra=if args.len()==6 {Some(fd(&args[5])?)} else {None};
    if extra.is_some_and(|e|[disk,client,relay].contains(&e)) {return Err(ErrorKind::InvalidInput.into());}
    let reopen=if let Some(extra)=extra {
        let listener=unsafe {UnixListener::from_raw_fd(extra)};
        listener.set_nonblocking(true)?;
        Some(Reopener {listener,previous:[0;16]})
    } else {None};
    drop(args);
    // Les trois descripteurs ouverts et distincts sont hérités du banc par pass_fds.
    unsafe {
        serve(
            File::from_raw_fd(disk),
            UnixStream::from_raw_fd(client),
            UnixStream::from_raw_fd(relay),
            sector,
            reopen,
        )
    }
}
#[cfg(test)]
mod tests;
