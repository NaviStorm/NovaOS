"""Démarrage séparé du vrai nsh ordinaire et de l'ELF @probe en cli-test, sur des P9 neufs."""
from laboratoire import *
from vecteurs import build
import sys,re,time,statistics,socket
from contextlib import ExitStack
from types import SimpleNamespace
sys.path.insert(0,str(ROOT/"novaOS/tests"))
from cli import HOST,Terminal
from preuves_stockage import figer_initiale
from journalisation import arret
from bloc import qmp
from pipe_natif import host

def inputs():
    rows=[f"ARC027 {i:03d} texte\n".encode() for i in range(128)]
    base=b"".join(rows)
    files={"base":base,"vecteurs":(HERE/"preuves/vecteurs.bin").read_bytes()}
    for name,idx in [("equal",[]),("start",[0]),("end",[127]),("many",range(0,128,17))]:
        f=rows.copy()
        for i in idx:f[i]=b"MOD "+f[i]
        files["diff-"+name]=b"".join(f)
    for t in ["treeA","treeB"]:
        files[t+"/part/k"]=base if t=="treeA" else files["diff-end"]
        files[t+"/first"]=b"commun\n"
    return files
def fixture(out,sector,files):
    commands=["mkdir /"+p for p in ["treeA","treeA/part","treeB","treeB/part"]]
    for name,data in files.items():
        commands.append("create /"+name)
        for i in range(0,len(data),4096):commands.append(f"write /{name} {i} {data[i:i+4096].hex()}")
    src=out/"genese.raw";image=out/"volume.raw"
    with src.open("xb") as f:
        r=subprocess.run([HOST,"script","512",str(sector)],input=("\n".join(commands)+"\n").encode(),stdout=f,stderr=subprocess.PIPE)
    if r.returncode:raise RuntimeError(r.stderr)
    fs,lv=re.search(rb"FS=(\w+) LV=(\w+)",r.stderr).groups()
    with src.open("rb") as f,image.open("xb") as dest:
        subprocess.run([HOST,"migrate-content-p9","512",str(sector),fs,lv],stdin=f,stdout=dest,check=True)
    figer_initiale(image);return image
def capture(vm,cmd):
    begin=time.monotonic_ns()
    vm.vm.stdin.write((cmd+"; echo DIFFRC_$?__fin\n").encode())
    data=vm.until(b"\r\nnsh> ",timeout=120)
    end=time.monotonic_ns()
    m=re.search(rb"DIFFRC_(\d+)__fin",data)
    return (int(m[1]) if m else None),data,end-begin
def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument("arch",choices=["aarch64","x86_64"]);p.add_argument("sector",type=int,choices=[512,4096]);p.add_argument("profile",choices=["ordinaire","recette"]);p.add_argument("out",type=Path);p.add_argument("--lab",type=Path);a=p.parse_args()
    out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    target="aarch64-unknown-none-softfloat" if a.arch=="aarch64" else "x86_64-unknown-none"
    os.environ.update(NOVA_VCPUS="2",NOVA_RAM="256",NOVAOS_TMPDIR=str(TMP),TMPDIR=str(TMP)+"/",RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
    root=a.lab if a.profile=="recette" else ROOT
    kernel=root/"novaOS/target"/target/"release/novaos";os.environ["NOVA_KERNEL_ELF"]=str(kernel)
    files=inputs();image=fixture(out,a.sector,files);series=out/"session.serial";results=[]
    expected={}
    with tempfile.TemporaryDirectory(dir=TMP,prefix="diff-oracle-invite-") as tmp:
        d=Path(tmp)
        for name,k in [("equal","equal"),("start","start"),("end","end"),("many","many"),("tree",None)]:
            if k:
                (d/"a").write_bytes(files["base"]);(d/"b").write_bytes(files["diff-"+k]);opts=["a","b"]
                size=len(files["base"])+len(files["diff-"+k])
            else:
                (d/"a").unlink();(d/"b").unlink()
                for key,data in files.items():
                    if key.startswith("tree"):
                        f=d/("a" if key.startswith("treeA") else "b")/key.split("/",1)[1];f.parent.mkdir(exist_ok=True,parents=True);f.write_bytes(data)
                opts=["-r","a","b"];size=sum(len(v) for k,v in files.items() if k.startswith("tree"))
            expected[name]=(execute(oracle(),opts,d),size)
    (ROOT/"novaOS/build").mkdir(exist_ok=True)
    with ExitStack() as stack:
        sock,peer=socket.socketpair();stack.enter_context(sock);stack.enter_context(peer)
        stream=stack.enter_context(sock.makefile("rwb",buffering=0));monitor=SimpleNamespace(stdin=stream,stdout=stream)
        vm=stack.enter_context(Terminal(a.arch,a.sector,image,"diff",test=a.profile=="recette",journal=series,qmp_bloc=peer.fileno()))
        peer.close();qmp(monitor,"qmp_capabilities");vm.until(b"nsh> ")
        if a.profile=="ordinaire":
            rc,log,_=capture(vm,"diff base diff-equal");assert rc==127,log;results.append(["diff base diff-equal",rc])
            rc,log,_=capture(vm,"cat base | diff - diff-equal");assert rc is None and "saisie refusée".encode() in log,log;results.append(["cat base | diff - diff-equal","refus syntaxique"])
            def disk():
                stats,=[v["stats"] for v in qmp(monitor,"query-blockstats") if v["device"]=="d"]
                return [digest(image),stats["wr_operations"],stats["flush_operations"]]
            before=disk()
            for _ in range(12):
                time.sleep(1);curr=disk()
                if curr==before:break
                before=curr
            for lang,fr in [("en",False),("fr-CA",True),("fr--CA",False)]:
                vm.line("export LANG="+lang);mark=("Comparer deux" if fr else "Compare two").encode()
                for opt in ["--help","--version"]:
                    rc,log,_=capture(vm,"diff "+opt)
                    assert rc==0 and (mark if opt=="--help" else b"diff (NovaOS) 0.24.0") in log,log
                    results.append([lang,opt,rc])
                vm.vm.stdin.write(b"man diff; echo DIFFRC_$?__fin\n")
                screen=vm.until(b"q quitter" if fr else b"q quit")
                assert mark in screen,screen
                vm.vm.stdin.write(b"q");vm.until(b"DIFFRC_0__fin");vm.until(b"nsh> ");results.append([lang,"man diff",0])
            after=disk();save(out/"disque-documentation.json",[before,after]);assert before==after
        else:
            rc,log,_=capture(vm,"@probe diff-corpus")
            m=re.search(rb"DIFF-VECTEURS total=(\d+) ecarts=(\d+)",log)
            assert m,log[-2000:];results.append(dict(cas=int(m[1]),ecarts=int(m[2]),statut=rc,divergences=[int(x) for x in re.findall(rb"DIFF-ECART id=(\d+)",log)]))
            for name,(exp,size) in expected.items():
                raw=[];peaks=[]
                for rep in range(4):
                    rc,log,ns=capture(vm,"@probe diff-"+name);parts=[bytearray(),bytearray()]
                    for side,chunk in re.findall(rb"DIFF-OCTETS ([01]) ([0-9a-f]+)",log):parts[int(side)].extend(bytes.fromhex(chunk.decode()))
                    got=[rc,parts[0].hex(),parts[1].hex()]
                    if got!=exp:raise RuntimeError((name,got,exp,log[-1200:]))
                    if rep:
                        raw.append(ns);peaks.append(int(re.search(rb"DIFF-PIC-TAS (\d+)",log)[1]))
                results.append(dict(charge=name,entree=size,statut=exp[0],octets_diff=len(exp[1])//2,stdout_sha256=hashlib.sha256(bytes.fromhex(exp[1])).hexdigest(),ns=raw,pic_tas_octets=peaks,mediane_mio_s=size/2**20/(statistics.median(raw)/1e9),mediane_ms=statistics.median(raw)/1e6))
        save(out/"resultats.json",results);arret(vm,2)
    if a.profile=="recette":shutil.copy2(a.lab/"probe.elf",out/"diff-recette.elf")
    print(a.arch,a.sector,a.profile,results)
    if a.profile=="recette" and (results[0]["ecarts"] or results[0]["statut"]):raise SystemExit(1)
if __name__=="__main__":main()
