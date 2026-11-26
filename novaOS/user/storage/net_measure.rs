#![no_std]
#![no_main]
#[path = "../../abi/files.rs"]
#[allow(dead_code)]
mod protocol;
mod runtime;
use protocol as w;
use runtime as rt;

mod fileapi;
use fileapi::Client;
fn log(s: &[u8]) {
    rt::call(rt::DEBUG_WRITE, s.as_ptr() as u64, s.len() as u64, 0);
}
fn ensure(v: bool) -> rt::Result<()> {
    if v { Ok(()) } else { Err(rt::REFUSED) }
}
#[cfg(net_p9_coexistence)]
fn coexistence(mut c: Client) -> rt::Result<()> {
    c.mount()?;
    let f = c.open(b"/sortie", w::READABLE | w::WRITABLE)?;
    let mut data=[0u8;512];
    let mut observation=[0u8;512];
    let debut=w::word(&c.info()?,w::PAYLOAD+72);
    let mut generation=debut;
    let mut lectures=0;
    for tour in 0u16..256 {
        for (i,b) in data.iter_mut().enumerate() {
            *b=(i as u8).wrapping_mul(73) ^ (tour as u8);
        }
        c.write(f,0,&data)?;
        let recu=c.sync(f)?;
        if recu!=generation+1 {log(b"NET P9 generation SYNC divergente");return Err(rt::REFUSED);}
        ensure(c.read_grouped(f,0,&mut observation)?==data.len() && observation==data)?;
        generation=w::word(&c.info()?,w::PAYLOAD+72);
        if generation!=recu && generation!=recu+1 {log(b"NET P9 generation READ groupee divergente");return Err(rt::REFUSED);}
        lectures+=generation-recu;
    }
    c.close(f)?;
    c.close(0)?;
    let mut texte=*b"NET P9 generations 0000000000000000 0000000000000000 0000000000000000";
    for (i,val) in [debut,generation,lectures].into_iter().enumerate() {
        for j in 0..16 {texte[19+i*17+j]=b"0123456789abcdef"[((val>>(60-j*4))&15) as usize];}
    }
    log(&texte);
    log(b"NET P9 256 transactions WRITE SYNC READ octets conformes");
    Ok(())
}


#[cfg(net_q35_p9)]
fn nvme(mut c: Client) -> rt::Result<()> {
    c.mount()?;
    let next: [u8; 4231] = core::array::from_fn(|i| (i as u8).wrapping_mul(73) ^ (i >> 5) as u8);
    let f = c.open(b"/sortie", w::READABLE | w::WRITABLE)?;
    log(b"NVME P9 debut transaction");
    let mut data=next;
    let mut actual=[0u8;4231];
    for tour in 0..128u8 {
        data[0]=127-tour;
        c.write(f,0,&data)?;
        c.sync(f)?;
        ensure(c.read(f,0,&mut actual)?==data.len() && actual==data)?;
    }
    ensure(data==next)?;
    log(b"NVME P9 128 transactions WRITE SYNC READ conformes");
    c.close(f)?;
    c.close(0)
}

#[cfg(net_q35_p9)]
fn cycle(mut c: Client) -> rt::Result<()> {
    c.mount()?;
    let f = c.open(b"/sortie", w::READABLE | w::WRITABLE)?;
    let mut previous = [0u8; 1];
    ensure(c.read(f, 0, &mut previous)? == 1)?;
    let mut data: [u8; 4231] = core::array::from_fn(|i| (i as u8).wrapping_mul(73) ^ (i >> 5) as u8);
    data[0] = u8::from(previous[0] != 1);
    log(b"NVME P9 debut transaction");
    c.write(f, 0, &data)?;
    c.sync(f)?;
    log(b"NVME P9 sauvegarde confirmee");
    let mut readback = [0u8; 4231];
    ensure(c.read(f, 0, &mut readback)? == data.len() && data == readback)?;
    log(b"NVME P9 cycle contenu conforme");
    c.close(f)?;
    c.close(0)
}

#[cfg(nvme_rw)]
#[path = "../nvme_diagnostic.rs"]
mod nvme_diagnostic;

fn probe() -> rt::Result<()> {
    match rt::config(1) {
        #[cfg(net_p9_coexistence)]
        25 => coexistence(Client::new()?),
        #[cfg(net_q35_p9)]
        1007 => nvme(Client::new()?),
        #[cfg(net_q35_p9)]
        1008 => cycle(Client::new()?),
        _ => Err(rt::REFUSED),
    }
}

#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(probe().err().unwrap_or(0))
}
