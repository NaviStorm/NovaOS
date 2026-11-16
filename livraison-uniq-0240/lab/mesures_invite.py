"""Mesure le traitement isolé d’un fichier P9 trié vers une destination neuve."""
import argparse, hashlib, json, os, re, subprocess, sys, time
from pathlib import Path
from bench import ROOT,corpus
sys.path.insert(0,str(ROOT.parent/"novaOS/tests"))
from cli import HOST,Terminal
from pipe_natif import host
from journalisation import arret

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"]);p.add_argument("sector",type=int,choices=[512,4096]);p.add_argument("out",type=Path)
    a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    os.environ.update(NOVA_RAM="256",NOVA_VCPUS="2")
    keys,counts,_=corpus(8192,32,64)
    sorted_data=b"".join(k*n for k,n in zip(keys,counts))
    data={"gros-fichier.txt":sorted_data,"court":(b"A"*31+b"\n")*128,"long":(b"L"*32767+b"\n")*4,"compteur":bytes(10_000_019)}
    deps=ROOT.parent/"novaFS/target/aarch64-apple-darwin/release/deps";lib,=deps.glob("libnovafs-*.rlib")
    generate=out/"image-p9";image=out/"session.raw";blocks="2048";sector=str(a.sector)
    subprocess.run(["rustup","run","stable","rustc","--edition=2024","-O",Path(__file__).with_name("image_p9.rs"),"-L","dependency="+str(deps),"--extern","novafs="+str(lib),"-o",generate],check=True)
    subprocess.run([generate,image,blocks,sector,"--mesures"],input=sorted_data,check=True)
    rows=[];report=out/"mesures.json"
    with Terminal(a.arch,a.sector,image,"uniq-mesures",test=True,journal=out/"mesures.serial") as vm:
        vm.until(b"nsh> ")
        for name,raw in data.items():
            for opt in (["-zc"] if name=="compteur" else ["-c","-D","--group"]):
                output="resultat"+str(len(rows));cmd=f"uniq {opt} {name} {output}"
                vm.vm.stdin.write(f"echo UBEGIN; {cmd}; echo URC_$?__fin\n".encode());vm.until(b"\r\nUBEGIN\r\n")
                start=time.perf_counter();body=vm.until(b"__fin\r\n",timeout=300);elapsed=time.perf_counter()-start;vm.until(b"nsh> ")
                rc=int(re.search(rb"URC_(\d+)__fin",body)[1])
                ref=subprocess.check_output(["/opt/homebrew/bin/guniq",opt],input=raw,env={**os.environ,"LC_ALL":"C"})
                rows.append(dict(cmd=cmd,name=output,option=opt,status=rc,bytes=len(raw),seconds=elapsed,mib_s=len(raw)/elapsed/(1<<20),
                                 lines_s=raw.count(0 if name=="compteur" else 10)/elapsed,expected_hex=ref.hex()))
                report.write_text(json.dumps(rows)+"\n")
        arret(vm,2)
    anchors=re.findall(rb"slot=(\d+) g\xc3\xa9n\xc3\xa9ration=(\d+)",host(image,"inspect",blocks,sector))
    slot=max(anchors,key=lambda r:int(r[1]))[0]
    for row in rows:
        if row["status"]==0:
            actual=host(image,"salvage",blocks,sector,"27"*16,"28"*16,slot,"/"+row["name"])
            row["actual_sha256"]=hashlib.sha256(actual).hexdigest()
            row["ok"]=actual.hex()==row["expected_hex"]
        else:row["ok"]=False
    report.write_text(json.dumps(rows,indent=1)+"\n")
    print(sum(r["ok"] for r in rows),"/",len(rows))
    raise SystemExit(not all(r["ok"] for r in rows))
if __name__=="__main__":main()
