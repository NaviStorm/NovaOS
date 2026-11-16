//! Prépare une image P9 neuve avec un fichier dense. Aucun accès à un disque brut.
use novafs::{device::FileImage, time::Atime, Engine, Ids, Mode};
use std::{env, fs::File, io::Write, time::Instant};

const ROW: usize=4096;
const CHUNK: usize=65536;
const SIZE: usize=320<<20;
const BLOCKS:u64=262144;
const NAME:&str="/corpus-sort";

fn fill(buf:&mut [u8],offset:usize,state:&mut u64) {
    for (n,row) in buf.chunks_exact_mut(ROW).enumerate(){
        *state=state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        row.fill(b'x');
        let mut head=&mut row[..34];
        write!(head,"{:016x} {:016x} ",*state,offset/ROW+n).unwrap();
        row[ROW-1]=b'\n';
    }
}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let mut args=env::args().skip(1);
    let path=args.next().ok_or("image de destination requise")?;
    let sector=args.next().ok_or("secteur requis")?.parse()?;
    let ids=Ids{fs:*b"NOVA-SORT-CORPUS",lv:*b"NOVA-SORT-VOLP09"};
    let file=File::options().read(true).write(true).create_new(true).open(path)?;
    file.set_len(BLOCKS*4096)?;
    let dev=FileImage::from_file(file,BLOCKS,sector)?;
    let mut fs=Engine::format_p9(dev,ids,Atime::Relative,None)?;
    fs.create(NAME)?;
    let start=Instant::now();let mut state=27;let mut block=[0;CHUNK];
    for at in (0..SIZE).step_by(CHUNK){
        fill(&mut block,at,&mut state);
        if let Err(e)=fs.write(NAME,at as u64,&block){
            eprintln!("P9 écriture à {at} octets : {e:?}");
            return Err(e.into())
        }
    }
    let generation=start.elapsed();
    let fs=Engine::mount_current(fs.unmount()?,ids,Mode::ReadOnly)?;
    let stat=fs.stat(NAME)?;assert_eq!(stat.size,SIZE as u64);
    let start=Instant::now();state=27;
    for at in (0..SIZE).step_by(CHUNK){
        fill(&mut block,at,&mut state);
        assert_eq!(fs.inspect_id(stat.id,at as u64,CHUNK)?,block);
    }
    println!("P9 secteur={sector} fichier={NAME} octets={SIZE} lignes={} écriture_s={:.3} relecture_s={:.3}",SIZE/ROW,generation.as_secs_f64(),start.elapsed().as_secs_f64());
    fs.unmount()?;
    Ok(())
}
