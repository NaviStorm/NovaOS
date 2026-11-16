"""Recette QEMU P9 depuis nsh, sur un répertoire de preuves neuf."""
import argparse, hashlib, json, os, re, shlex, shutil, subprocess, sys, time
from pathlib import Path
from options import BASE, OPTIONS, GNU
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import Terminal
from pipe_natif import fixture, host
from journalisation import arret
from pipe_head import rendered

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"]);p.add_argument("sector",type=int,choices=[512,4096])
    p.add_argument("out",type=Path);p.add_argument("--sort",action="store_true")
    p.add_argument("--doc-kernel",type=Path,help="Noyau distinct construit avec NOVA_DOC_TRACE=1")
    p.add_argument("--rows",type=int,default=768)
    a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    os.environ.update(NOVA_RAM="256",NOVA_VCPUS="2")
    from bench import corpus
    keys,frequency,order=corpus(a.rows,32,64)
    large=b"".join(keys[i] for i in order)
    expected=b"".join(f"{n:7d} ".encode()+k for k,n in zip(keys,frequency) if n)
    files={"cities":BASE,"fields":b" aa\tXY0\nbb\tXY1\ncc\tXZ1\nsolo\n","empty":b"",
           "binary":b"q\nq\0q\nq\0X\xff\0X\xff","long":b"p"*2600+b"\n"+b"p"*2600,
           "gros-fichier.txt":large,"utf":"éX\nßX\nÉ\né\n".encode(),"-villes":b"Madrid\nLisbon\nMadrid\n"}
    (out/"corpus.json").write_text(json.dumps(dict(seed=27027,rows=a.rows,keys=len(keys),width=32,bytes=len(large),frequencies=frequency,ram_mib=256,input_sha256=hashlib.sha256(large).hexdigest(),expected_sha256=hashlib.sha256(expected).hexdigest()),indent=1))
    for i in range(8):
        import random
        rng=random.Random(i)
        files[f"seed{i}"]=b"\n".join(rng.choice([b"x",b"X",b"a"*35,b"\xffX"]) for _ in range(20))
    reference=subprocess.run(["/opt/homebrew/bin/gsort"],input=large,capture_output=True,check=True,env={**os.environ,"LC_ALL":"C"})
    files["trie"]=reference.stdout
    assert subprocess.check_output([GNU,"-c"],input=files["trie"])==expected
    image,fs,lv=fixture(out,a.sector,files)
    all_rows=[]
    def save():
        (out/"matrice.json").write_text(json.dumps(all_rows,indent=1,ensure_ascii=False)+"\n")
    def run(vm,cmd,status=0,want=None,pipe=None):
        started=time.perf_counter()
        vm.vm.stdin.write(f"echo UBEGIN; {cmd}; echo URC_$?__fin\n".encode())
        vm.until(b"\r\nUBEGIN\r\n")
        raw=b"\r\n"
        if cmd=="man uniq":
            raw+=vm.until(b"man uniq 1/")
            vm.vm.stdin.write(b"q")
        if cmd=="uniq -c -":
            vm.vm.stdin.write(b"STDIN\\nSTDIN\\n".replace(b"\\n",b"\n")+bytes([4]))
        raw+=vm.until(b"__fin\r\n",timeout=240)
        vm.until(b"nsh> ")
        raw=raw.replace(b"\r\n",b"\n")
        body=raw.split(b"\n",1)[1]
        rc=int(re.search(rb"URC_(\d+)__fin",body)[1])
        body=body[:body.rindex(b"URC_")]
        # Les diagnostics noyau de ce profil sont multiplexés sur la même UART.
        body=re.sub(rb"(?:DEBUG pid=\d+ TMP retrait confirme|TMP retrait instance=\d+ statut=0)\n",b"",body)
        ok=rc==status and (want is None or body==want)
        row=dict(cmd=cmd,seconds=time.perf_counter()-started,status=rc,expected_status=status,actual_hex=body.hex(),expected_hex=None if want is None else want.hex(),passed=ok)
        if pipe:
            row["stages"]={k.decode():int(v) for k,v in re.findall(rb"\[pipe ([a-z]+)=(\d+)\]",raw)}
            row["passed"] &= row["stages"]==pipe
        all_rows.append(row);save()
        return body
    serial=out/"ordinaire.serial"
    with Terminal(a.arch,a.sector,image,"uniq",journal=serial) as vm:
        vm.until(b"nsh> ")
        for opt in OPTIONS:
            if opt in ["-","- -","a b c"]:continue
            args=opt.split();name="fields" if any(x.startswith(("-f","--skip-f")) for x in args) else "cities"
            ref=subprocess.run([GNU,*args],input=files[name],capture_output=True,env={**os.environ,"LC_ALL":"C"})
            run(vm,shlex.join(["uniq",*args,name]),0 if ref.returncode==0 else 3,rendered(ref.stdout) if ref.returncode==0 else None)
        for args,name in [(["-c"],"long"),(["-c"],"empty"),(["--"],"-villes"),(["-z","--group=both"],"binary")]:
            r=subprocess.check_output([GNU,*args],input=files[name],env={**os.environ,"LC_ALL":"C"})
            run(vm,shlex.join(["uniq",*args,name]),want=rendered(r))
        for i,flag in enumerate(["-ci","--group=both","-D","-u","-s1","-w1","--all-repeated=separate","-id"]):
            name=f"seed{i}";ref=subprocess.check_output([GNU,flag],input=files[name],env={**os.environ,"LC_ALL":"C"})
            run(vm,"uniq "+flag+" "+name,want=rendered(ref))
        for loc in ["C","en_US.UTF-8"]:
            run(vm,"export LC_ALL="+loc)
            for flag in ["-s1","-w1","-i"]:
                expected_utf=subprocess.check_output([GNU,flag],input=files["utf"],env={**os.environ,"LC_ALL":loc})
                run(vm,"uniq "+flag+" utf",want=rendered(expected_utf))
        run(vm,"unset LC_ALL")
        run(vm,"export _POSIX2_VERSION=199209")
        run(vm,"uniq +1 cities",want=subprocess.check_output([GNU,"-s1"],input=BASE))
        run(vm,"unset _POSIX2_VERSION")
        run(vm,"uniq -c -",want=b"      2 STDIN\n")
        run(vm,"uniq -z --group=both binary sortie",want=b"")
        run(vm,"uniq cities cities",status=27)
        run(vm,"uniq absent",status=20)
        run(vm,"uniq ../cities",status=5)
        run(vm,"uniq cities ../sortie",status=5)
        run(vm,"cat cities | uniq -c",want=subprocess.check_output([GNU,"-c"],input=BASE))
        run(vm,"cat cities | uniq cities",3)
        run(vm,"nsh -c 'uniq cities | wc -l'",3)
        run(vm,"cat cities | wc -l",want=f"{BASE.count(bytes([10]))}\n".encode())
        run(vm,"cat cities | head -n 1",want=b"madrid\n")
        run(vm,"uniq -c trie",want=expected)
        if a.sort:
            run(vm,"sort gros-fichier.txt | uniq -c",want=expected)
        arret(vm,2)
    if a.doc_kernel:os.environ["NOVA_KERNEL_ELF"]=str(a.doc_kernel.resolve(strict=True))
    with Terminal(a.arch,a.sector,image,"uniq-droits",test=True,journal=out/"droits.serial") as vm:
        vm.until(b"nsh> ")
        for tag,word in [("fr-CA","Filtrer"),("fr-FAUX-000","Filter"),("en","Filter")]:
            run(vm,"export LANG="+tag)
            # Laisser le journal de démarrage finir sa publication avant la fenêtre documentaire.
            for _ in range(20):
                before=hashlib.sha256(image.read_bytes()).hexdigest()
                time.sleep(.25)
                if hashlib.sha256(image.read_bytes()).hexdigest()==before:break
            for cmd in ["uniq --help","man uniq","@probe documentation uniq --help","@probe documentation uniq --version","uniq --version"]:
                body=run(vm,cmd)
                ok=(word.encode() if "version" not in cmd else b"0.24.0") in body
                if cmd.startswith("@probe") and a.doc_kernel:ok &= b"business=0" in body
                if not ok:all_rows[-1]["passed"]=False
            after=hashlib.sha256(image.read_bytes()).hexdigest()
            all_rows.append(dict(cmd="P9 documentation "+tag,before=before,after=after,passed=before==after))
        body=run(vm,"cat cities | uniq -c",pipe={"cat":0,"uniq":0})
        all_rows[-1]["passed"] &= b"PIPE admission image=17 fichiers=0 terminal=6" in body and b"PIPE retrait noeuds=0 attentes=0 file=0" in body
        if a.sort:run(vm,"sort gros-fichier.txt | uniq -c",pipe={"sort":0,"uniq":0})
        run(vm,"nsh -c 'sort cities | head -n 1'",3)
        run(vm,"cat cities | uniq cities",3)
        arret(vm,2)
    for name,expected_bytes in [("cities",BASE),("sortie",subprocess.check_output([GNU,"-z","--group=both"],input=files["binary"]))]:
        records=re.findall(rb"slot=(\d+) g\xc3\xa9n\xc3\xa9ration=(\d+)",host(image,"inspect","512",str(a.sector)))
        slot=max(records,key=lambda r:int(r[1]))[0]
        actual=host(image,"salvage","512",str(a.sector),fs,lv,slot,"/"+name)
        all_rows.append(dict(cmd="lecture P9 "+name,actual_hex=actual.hex(),expected_hex=expected_bytes.hex(),passed=actual==expected_bytes))
    # Le noyau ordinaire confirme la terminaison du client et le retrait ; les
    # compteurs détaillés ne sont publiés que dans le profil de qualification.
    ordinary=(out/"ordinaire.serial").read_bytes()
    all_rows.append(dict(cmd="arrêt ordinaire",passed=b"BLOC session terminee ok=true" in ordinary and b"retrait=Ok(())" in ordinary))
    for session in ["droits"]:
        serial=(out/(session+".serial")).read_bytes()
        line=re.search(rb"^RAM phase=bloc_collecte .*",serial,re.M)
        resources={} if line is None else dict((k.decode(),int(v)) for k,v in re.findall(rb"\b(ipc|utilisateur|processus|dma)=(\d+)",line[0]))
        all_rows.append(dict(cmd="collecte "+session,resources=resources,passed=all(resources.get(k)==0 for k in ["utilisateur","processus","ipc"])))
    save()
    bad=[r["cmd"] for r in all_rows if not r["passed"]]
    print(len(all_rows)-len(bad),"/",len(all_rows),bad)
    raise SystemExit(bool(bad))
if __name__=="__main__":main()
