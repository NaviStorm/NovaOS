"""Profil supérieur à 256 Mio de RAM, avec corpus dense sur un vrai volume P9."""
import argparse,hashlib,json,os,re,shutil,subprocess,sys,time
from pathlib import Path
from bench import corpus,hashfile,ROOT
sys.path.insert(0,str(ROOT.parent/"novaOS/tests"))
from cli import HOST,Terminal
from journalisation import arret

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"]);p.add_argument("sector",type=int,choices=[512,4096])
    p.add_argument("out",type=Path);p.add_argument("--large",action="store_true",required=True)
    p.add_argument("--rows",type=int,default=262145)
    a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    os.environ.update(NOVA_RAM="256",NOVA_VCPUS="2")
    rows=a.rows;width=1024;size=rows*width
    blocks=(size*4)//4096+2048
    keys,freq,order=corpus(rows,width,64)
    initial_free=shutil.disk_usage(out).free
    if initial_free<blocks*4096*4:raise OSError("espace insuffisant")
    expected=b"".join(f"{v:7d} ".encode()+k for k,v in zip(keys,freq))
    (out/"attendu").write_bytes(expected)
    sha=hashlib.sha256()
    image=out/"session.raw"
    source=out/"gros-fichier.txt"
    with source.open("xb") as f:
        for i in order:
            row=keys[i];sha.update(row);f.write(row)
    generate=out/"image-p9"
    deps=ROOT.parent/"novaFS/target/aarch64-apple-darwin/release/deps"
    lib,=deps.glob("libnovafs-*.rlib")
    subprocess.run(["rustup","run","stable","rustc","--edition=2024","-O",Path(__file__).with_name("image_p9.rs"),
        "-L","dependency="+str(deps),"--extern","novafs="+str(lib),"-o",generate],check=True)
    with source.open("rb") as src,(out/"genese.txt").open("wb") as log:
        subprocess.run([generate,image,str(blocks),str(a.sector)],stdin=src,stdout=log,stderr=log,check=True)
    with subprocess.Popen(["/opt/homebrew/bin/gsort",source],stdout=subprocess.PIPE,env={**os.environ,"LC_ALL":"C"}) as sort:
        reference=subprocess.check_output(["/opt/homebrew/bin/guniq","-c"],stdin=sort.stdout)
        sort.stdout.close();assert sort.wait()==0 and reference==expected
    meta=dict(rows=rows,width=width,bytes=size,ram_mib=256,blocks=blocks,sector=a.sector,keys=64,seed=27027,frequencies=freq,
              input_sha256=sha.hexdigest(),expected_sha256=hashlib.sha256(expected).hexdigest(),avant_sha256=hashfile(image),free_before=initial_free)
    result=out/"grand.json"
    result.write_text(json.dumps(meta,indent=1))
    start=time.monotonic()
    with Terminal(a.arch,a.sector,image,"uniq-grand",test=True,journal=out/"grand.serial") as vm:
        try:
            vm.until(b"nsh> ",timeout=1200,refus=b"BLOC session terminee ok=false")
            vm.vm.stdin.write(b"echo UBEGIN; sort gros-fichier.txt | uniq -c; echo URC_$?__fin\n")
            vm.until(b"\r\nUBEGIN\r\n")
            started=time.monotonic()
            body=vm.until(b"__fin\r\n",timeout=600).replace(b"\r\n",b"\n")
            meta.update(seconds=time.monotonic()-started,sort=int(re.search(rb"\[pipe sort=(\d+)\]",body)[1]),
                        uniq=int(re.search(rb"\[pipe uniq=(\d+)\]",body)[1]))
            lines=re.findall(rb"^ *[0-9]+ [0-9a-f]{8}q+\n",body,re.M)
            meta["observed_sha256"]=hashlib.sha256(b"".join(lines)).hexdigest()
            meta["ok"]=b"".join(lines)==expected and meta["sort"]==meta["uniq"]==0
            vm.until(b"nsh> ");arret(vm,2)
        except (RuntimeError,TypeError) as error:
            meta.update(ok=False,erreur=str(error)[-1400:],elapsed=time.monotonic()-start)
    result.write_text(json.dumps(meta,indent=1)+"\n")
    print(meta.get("sort"),meta.get("uniq"),meta["ok"])
    raise SystemExit(0 if meta["ok"] else 1)
if __name__=="__main__":main()
