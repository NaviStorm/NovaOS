"""Exécuter la recette privée de l’ELF et la documentation du vrai nsh."""
import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from banc import BASE, TMP, FIELDS

ROOT=BASE.parent
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import Terminal
from pipe_natif import fixture
from journalisation import arret

def patch(p,old,new):
    s=p.read_text()
    assert s.count(old)==1,(p,old)
    p.write_text(s.replace(old,new))

def overlay(dest):
    # Le remplacement de ls existe exclusivement dans la copie de recette.
    for name in ["components","novaOS","novaFS"]:
        if name=="novaFS":
            (dest/name).symlink_to(ROOT/name,target_is_directory=True)
        else:
            shutil.copytree(ROOT/name,dest/name,ignore=shutil.ignore_patterns("target","build",".DS_Store"))
    shell=dest/"components/shell"
    patch(shell/"script.rs","            _ if let Some(p) = Program::named",
          '            "@stat" => executor::recipe_stat(self.fs,&args)?,\n            _ if let Some(p) = Program::named')
    with (shell/"executor.rs").open("a") as f:
        f.write("""
#[path="../stat/src/options.rs"]
mod stat_options;
pub fn recipe_stat(fs:&mut Client,args:&[String])->rt::Result<u64>{
    let raw:Vec<_>=args.iter().map(|s|s.as_bytes()).collect();
    let mut grant=Grant::new();
    let mut prep=0;
    if let Ok(o)=stat_options::parse(&raw,false) && o.doc.is_none() && !o.fs && !o.files.contains(&b"-".as_slice()) {
        let mut words=alloc::vec!["--"];
        for path in o.files{words.push(core::str::from_utf8(path).map_err(|_|rt::BAD_ARGUMENT)?)}
        prep=operand(Program::Ls,&words).and_then(|op|grant.select(fs,&op)).err().unwrap_or(0);
    }
    let channels=if prep==0{grant.channels}else{(0,0)};
    let status=process::spawn(Program::Ls,args,channels,prep).and_then(process::wait);
    grant.release(fs)?;
    let (fault,code)=status?;
    Ok(if fault{128}else{code})
}
""")
    patch(dest/"novaOS/kernel/src/sched/process.rs","        stop_process(s, pid, result, entered);",
          '''        if p.terminal != 0 && let Some(space)=p.space.as_ref() {
            let (data,stack)=space.page_counts();
            hal::log(format_args!("STAT_RECETTE_PAGES {}",data+stack));
        }
        stop_process(s, pid, result, entered);''')
    patch(dest/"novaOS/kernel/build.rs",
          'std::fs::copy(work.join(&target).join("release").join(name), &path).unwrap();',
          '''if name=="ls" {std::fs::copy(env::var_os("STAT_RECIPE_ELF").unwrap(), &path).unwrap();}
          else {std::fs::copy(work.join(&target).join("release").join(name), &path).unwrap();}''')

def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()

def cell(arch,sector,kernel,out,elf,oracle,full):
    out.mkdir(parents=True)
    samples=[]
    with tempfile.TemporaryDirectory(dir=TMP,prefix="stat-p9-") as work:
        work=Path(work)
        count=64 if full else 8
        image,_,_=fixture(work,sector,{f"f{i}":b"abc" for i in range(count)})
        subprocess.run([oracle,"initialiser",image,str(sector)],check=True)
        observed=subprocess.check_output([oracle,"stat",image,str(sector),"/alpha"]).decode().split()
        assert observed[4]=="978307200.800000001"
        with gzip.open(out/"p9-initial.raw.gz","wb") as dst:dst.write(image.read_bytes())
        os.environ.update(NOVA_KERNEL_ELF=str(kernel),NOVA_ARCH=arch,NOVA_VCPUS="1",NOVA_RAM="256")
        log=work/"terminal.serial"
        with Terminal(arch,sector,image,"stat-recette",journal=log) as vm:
            vm.until(b"nsh> ")
            def call(cmd,rc,expect=None):
                start=time.perf_counter_ns()
                vm.vm.stdin.write((cmd+"\n").encode())
                raw=vm.until(b"\r\nnsh> ")
                ms=(time.perf_counter_ns()-start)/1e6
                vm.vm.stdin.write(b"echo STAT_RC_$?__\n")
                status=vm.until(b"\r\nnsh> ")
                assert f"STAT_RC_{rc}__\r\n".encode() in status,(cmd,raw[-500:],status[-300:])
                if expect:assert expect in raw,(cmd,raw[-1000:],expect)
                samples.append(dict(cmd=cmd,rc=rc,uart=raw.hex(),ms=ms))
                return raw
            call("stat f0",127)
            call("stat f0 | wc -l",3)
            call("@stat f0",0,b"regular file")
            call("@stat -c '%i %s %.9Y' alpha",0,(" ".join([observed[0],observed[1],observed[4]])+"\r\n").encode())
            call("@stat -c%y alpha",0,b"2001-01-01 00:00:00.800000001 +0000")
            call("@stat -c%n -",25)
            call("@stat --printf='\\0' f0",25)
            call("@stat -- -Z",20)
            call("@stat -c '%n=%s/%F' f0 f1",0,b"f0=3/regular file\r\nf1=3/regular file\r\n")
            call("@stat -c '%F' .",0,b"directory\r\n")
            call("@stat --dereference --terse --format='%a:%u:%s' f0",0,b"?:?:3\r\n")
            for mode in ["always","never","default"]:
                call(f"@stat --cached={mode} -c%s f0",0,b"3\r\n")
            call("@stat -Lf --printf=%T f0",25)
            call("@stat --file-system f0",25)
            call("@stat -c%n absent",20)
            call("@stat -Z",3)
            call("@stat --cached=oops f0",3)
            call("@stat --printf='%n=%s' -- f0",0,b"f0=3")
            for k in FIELDS:
                expected={"n":"f0","N":"f0","Qn":"f0","s":"3","F":"regular file","i":"2","w":"-","W":"0"}.get(k,"?")
                call(f"@stat -c 'champ=%{k}' f0",0,("champ="+expected+"\r\n").encode())
            for k in "a b c d f i l n Qn s S t T".split():
                call(f"@stat -fc%{k} f0",25)
            call("@stat -t f0",0)
            call("@stat --terse f0",0)
            call("@stat --help",0,b"Display file")
            call("@stat --version",0,b"0.24.0")
            call("export LANG=fr-CA",0)
            time.sleep(2)
            h=digest(image)
            call("stat --help",0,"Afficher les".encode())
            call("stat --version",0,b"0.24.0")
            vm.vm.stdin.write(b"man stat\n")
            manual=vm.until(b"man stat 1/")
            assert "Afficher les".encode() in manual
            vm.vm.stdin.write(b"q")
            vm.until(b"nsh> ")
            assert digest(image)==h
            call("export LANG=fr-zzzzz",0)
            call("stat --help",0,b"Display file")
            perf={}
            for kind,paths in [("distincts",[f"f{i}" for i in range(count)]),("répété",["f0"]*count)]:
                times=[]
                memory=[]
                for _ in range(5):
                    total=0
                    for i in range(0,count,8):
                        cmd="@stat -c%s "+" ".join(paths[i:i+8])
                        outbytes=call(cmd,0)
                        assert outbytes.splitlines().count(b"3")==8
                        total+=samples[-1]["ms"]
                        pages=re.findall(rb"STAT_RECETTE_PAGES (\d+)",outbytes)
                        assert len(pages)==1,pages
                        memory.append(int(pages[0])*4096)
                    times.append(total)
                perf[kind]=dict(octets_prives_sortie=max(memory),premier_ms=times[0],mediane_chaud_ms=statistics.median(times[1:]),brut_ms=times,objets=count)
            arret(vm,1)
        for name,src in [("terminal.iso.gz",log.with_suffix(".iso")),("p9-final.raw.gz",image)]:
            with src.open("rb") as source,gzip.open(out/name,"wb") as dest:shutil.copyfileobj(source,dest)
        shutil.copyfile(log,out/"terminal.serial")
        for p in work.glob("*.json"):shutil.copyfile(p,out/p.name)
        (out/"mesures.json").write_text(json.dumps(dict(arch=arch,secteur=sector,ram_Mio=256,vcpu=1,p9_docs=h,kernel=digest(kernel),elf=digest(elf),iso=digest(log.with_suffix(".iso")),performances=perf,essais=samples),ensure_ascii=False,indent=1))
        print(arch,sector,perf,flush=True)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("sortie",type=Path)
    p.add_argument("--rapide",action="store_true")
    a=p.parse_args()
    elf=BASE/f"bin/stat-{a.arch}.elf"
    target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[a.arch]
    env=dict(os.environ,RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip(),CARGO_NET_OFFLINE="true",TMPDIR=str(TMP)+"/",STAT_RECIPE_ELF=str(elf))
    cargo=["rustup","run","stable","cargo","build","--offline","--locked","--release","--target"]
    subprocess.run([*cargo,"aarch64-apple-darwin","--bin","novafs"],cwd=ROOT/"novaFS",env={**env,"CARGO_TARGET_DIR":str(ROOT/"novaFS/target")},check=True)
    (ROOT/"novaOS/build").mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(dir=TMP,prefix="stat-recette-") as temp:
        work=Path(temp);overlay(work)
        targetdir=TMP/"stat-arc027"/("noyau-"+a.arch)
        subprocess.run([*cargo,target],cwd=work/"novaOS",env={**env,"CARGO_TARGET_DIR":str(targetdir)},check=True)
        kernel=targetdir/target/"release/novaos"
        assert elf.read_bytes() in kernel.read_bytes()
        oracle=work/"oracle"
        lib=ROOT/"novaFS/target/aarch64-apple-darwin/release"
        subprocess.run([*cargo,"aarch64-apple-darwin","--lib"],cwd=ROOT/"novaFS",env={**env,"CARGO_TARGET_DIR":str(ROOT/"novaFS/target")},check=True)
        subprocess.run([env["RUSTC"],"--edition=2024","-L","dependency="+str(lib/"deps"),"--extern","novafs="+str(lib/"libnovafs.rlib"),ROOT/"novaOS/tests/cli_temps_oracle.rs","-o",oracle],env=env,check=True)
        for sector in [512,4096]:cell(a.arch,sector,kernel,a.sortie/str(sector),elf,oracle,not a.rapide)

if __name__=="__main__":main()
