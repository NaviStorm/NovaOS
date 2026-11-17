"""Qualification de l'ELF livré avec délégation historique dans une copie instrumentée."""
import re
import hashlib
import os
from pathlib import Path
import shutil
import statistics
import subprocess as sp
import time
from invite import ROOT, OS, HERE, TARGETS, Terminal, sortie, sha, iso_check

FILES={"valides":b"2024-02-29T12:34:56Z\n"*32,
       "invalides":b"date-invalide\n"*32,
       "reference":b"objet de recette\n",
       "entree":b"@0\n@1\n",
       "flux":b"@0\n@1\n"}

def prepare_lot(work,arch):
    tree=work/"lot-src";tree.mkdir()
    (tree/"novaFS").symlink_to(ROOT/"novaFS",target_is_directory=True)
    comps=tree/"components"
    shutil.copytree(ROOT/"components",comps,ignore=shutil.ignore_patterns("target",".DS_Store"))
    kernel=tree/"novaOS";kernel.mkdir()
    for p in OS.iterdir():
        if p.name=="kernel":
            shutil.copytree(p,kernel/"kernel",ignore=shutil.ignore_patterns("target"))
        elif p.name not in ("build","target"):
            (kernel/p.name).symlink_to(p,target_is_directory=p.is_dir())
    # Réutilisation des délégations / flux du banc. Aucune modification du noyau
    # livré. La sélection de fichier de nsh précède la substitution d'argv.
    p=comps/"libraries/process.rs"
    s=p.read_text()
    key="    let mut data = vec![0u8; tty::HEADER];"
    assert s.count(key)==1
    p.write_text(s.replace(key,'''    let selected_args;
    let args = if p == Program::Cat && args.len() == 1 {
        let path = &args[0];
        let flag = if path=="/reference-neuve" || path=="/alpha" {"--reference"} else if path=="/reference" {"-r"} else if path=="/entree" {"--file"} else {"-f"};
        let fmt = if path == "/entree" {"+@%s"} else if path == "/reference-neuve" || path == "/alpha" {"+%s.%N"} else {"+%s"};
        selected_args = vec![flag.into(), path.clone(), fmt.into()];
        &selected_args[..]
    } else if p == Program::Wc && pipe.is_some() {
        selected_args = vec!["--file=-".into(), "+%s".into()];
        &selected_args[..]
    } else {args};
'''+key))
    p=kernel/"kernel/build.rs";s=p.read_text()
    key='std::fs::copy(work.join(&target).join("release").join(name), &path).unwrap();'
    assert s.count(key)==1
    p.write_text(s.replace(key,'''if matches!(name,"cat"|"wc") {
            std::fs::copy(std::env::var_os("DATE_TEST_ELF").unwrap(), &path).unwrap();
        } else {'''+key+'}'))
    process=kernel/"kernel/src/sched/process.rs"
    source=process.read_text().replace("    exit_counter: u64,", "    date_peak: usize,\n    date_start: u64,\n    exit_counter: u64,").replace("            exit_counter: 0,", "            date_peak: 0,\n            date_start: 0,\n            exit_counter: 0,")
    process.write_text(source)
    gate=kernel/"kernel/src/sched/process/syscall.rs"
    source=gate.read_text()
    anchor="    let [a, b, c] = args;"
    assert source.count(anchor)==1
    gate.write_text(source.replace(anchor,'''    let date_sample = with(|s| {
        let pid=caller(s);
        let p=s.processes.get_mut(pid);
        if p.terminal != (crate::terminal::OUTPUT | crate::terminal::ERROR) {return None}
        let space=p.space.as_ref().unwrap();
        let pages=space.page_counts();
        p.date_peak=p.date_peak.max(pages.0+pages.1);
        let ticks=hal::timer::counter();
        if p.date_start==0 {p.date_start=ticks}
        if id==PROCESS_EXIT {
            let argv=heap::with_pages(|m|Ok(space.read_data(m,5))).unwrap().to_le_bytes();
            if argv[..5]==[3,2,0,b'-',b'f'] {
                return Some((pid.number,p.date_peak,ticks-p.date_start));
            }
        }
        None
    });
    if let Some((pid,pages,ticks))=date_sample {
        // La calibration TSC attend des interruptions PIT sur x86_64.
        // Effectuer cette opération une fois, hors de la section ordonnanceur.
        use core::sync::atomic::{AtomicU64,Ordering};
        static DATE_HZ:AtomicU64=AtomicU64::new(0);
        let mut hz=DATE_HZ.load(Ordering::Relaxed);
        if hz==0 {hz=hal::timer::counter_hz();DATE_HZ.store(hz,Ordering::Relaxed)}
        hal::log(format_args!("DATEMEM {} {} {} {}",pid,pages,ticks,hz));
    }
'''+anchor))
    elf=HERE/f"bin/date-{arch}.elf"
    target=work/"cible-lot"
    sp.run(["rustup","run","stable","cargo","build","--offline","--release","--locked","--target",TARGETS[arch]],
           cwd=kernel,env=dict(os.environ,CARGO_TARGET_DIR=str(target),DATE_TEST_ELF=str(elf)),check=True)
    image=target/TARGETS[arch]/"release/novaos"
    assert elf.read_bytes() in image.read_bytes()
    return image

def lot(arch,sector,work,image,kernel):
    os.environ["NOVA_KERNEL_ELF"]=str(kernel)
    result=[]
    with Terminal(arch,sector,image,"date-lot",journal=work/"lot.serial") as vm:
        vm.until(b"nsh> ")
        for name,line,status in [("valides","1709210096",0),("invalides","date: invalid date 'date-invalide'",1)]:
            times=[];metrics=[]
            for tour in range(4):
                start=time.monotonic_ns()
                response=sortie(vm,"cat /"+name,status)
                elapsed=time.monotonic_ns()-start
                assert response.splitlines().count(line)==32,(name,response[-1000:])
                times.append(elapsed/1e9)
                m=re.search(r"DATEMEM (\d+) (\d+) (\d+) (\d+)",response)
                assert m,response[-1500:]
                pid,pages,ticks,hz=map(int,m.groups())
                metrics.append(dict(pid=pid,pages=pages,octets=4096*pages,ticks=ticks,hz=hz,secondes=ticks/hz))
            result.append(dict(charge=name,dates=32,octets=len(FILES[name]),sha256=hashlib.sha256(FILES[name]).hexdigest(),
                               processus=metrics,echauffement=times[0],secondes=times[1:],mediane=statistics.median(times[1:]),
                               debit=32/statistics.median(times[1:]),statut=status))
        stamp=sortie(vm,"cat /reference",1)
        # La migration P9 peut conserver un horodatage inconnu.
        assert "modification time unknown" in stamp
        before=int(time.time())-2
        sortie(vm,"touch /reference-neuve",0)
        fresh=sortie(vm,"cat /reference-neuve",0)
        match=re.search(r"(?m)^(\d+)\.(\d{9})\s*$",fresh)
        assert match and before<=int(match[1])<=int(time.time())+2,fresh
        exact=sortie(vm,"cat /alpha",0)
        assert "978307200.800000001" in exact.splitlines(),exact
        flux=sortie(vm,"cat /entree | wc -l")
        assert "0\r\n1\r\n" in flux,flux
        result.extend([dict(cas="mtime",sortie=stamp),dict(cas="mtime-connue",sortie=fresh,intervalle=[before,int(time.time())+2]),dict(cas="mtime-ns",sortie=exact,attendu="978307200.800000001"),dict(cas="pipe",sortie=flux)])
    return dict(media=iso_check(work/"lot.iso",kernel,work),elf=sha(HERE/f"bin/date-{arch}.elf"),cas=result)
