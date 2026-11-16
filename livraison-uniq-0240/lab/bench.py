"""Corpus déterministe à effectifs connus. Mesures murales et RSS Darwin via wait4."""
import argparse,bisect,hashlib,itertools,json,math,os,shutil,subprocess,tempfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
SEED=27027

def corpus(rows,width,count):
    if not 3<=count<=rows//2 or width<10:raise ValueError("dimensions du corpus")
    keys=[f"{i:08x}".encode()+b"q"*(width-9)+b"\n" for i in range(count)]
    freq=[rows//2]+[1]*(count-1)
    for j in range(rows-sum(freq)):freq[2+j%(count-2)]+=1
    limits=list(itertools.accumulate(freq));step=rows//2+SEED
    while math.gcd(step,rows)!=1:step+=1
    return keys,freq,(bisect.bisect_right(limits,(SEED+i*step)%rows) for i in range(rows))

def hashfile(path):
    with path.open("rb") as f:return hashlib.file_digest(f,"sha256").hexdigest()

def measure(commands,path,out,tmp,rows):
    env={**os.environ,"LC_ALL":"C","TMPDIR":str(tmp)}
    start=time.perf_counter();children=[];stats={};peak=0
    with out.open("wb") as dest:
        for i,cmd in enumerate(commands):
            child=subprocess.Popen(cmd,stdin=children[-1].stdout if children else subprocess.DEVNULL,
                stdout=dest if i==len(commands)-1 else subprocess.PIPE,env=env)
            if children:children[-1].stdout.close()
            children.append(child)
        while len(stats)<len(children):
            try:peak=max(peak,sum(f.stat().st_blocks*512 for f in tmp.iterdir()))
            except FileNotFoundError:pass
            for c in children:
                if c.pid in stats:continue
                pid,status,ru=os.wait4(c.pid,os.WNOHANG)
                if pid:
                    c.returncode=os.waitstatus_to_exitcode(status)
                    stats[pid]=dict(status=c.returncode,rss_bytes=ru.ru_maxrss,cpu_s=ru.ru_utime+ru.ru_stime)
            if len(stats)<len(children):time.sleep(0.005)
    elapsed=time.perf_counter()-start
    return dict(commands=[[str(a) for a in cmd] for cmd in commands],processes=[stats[c.pid] for c in children],seconds=elapsed,
                mib_s=path.stat().st_size/(1<<20)/elapsed,lines_s=rows/elapsed,temp_peak_sampled_bytes=peak)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("mode",choices=["pipeline","mesures"])
    p.add_argument("target",choices=["GNU","NovaOS"]);p.add_argument("--large",action="store_true")
    p.add_argument("--sort",type=Path);p.add_argument("--output",type=Path)
    a=p.parse_args()
    gsort=Path("/opt/homebrew/bin/gsort");guniq=Path("/opt/homebrew/bin/guniq")
    sort=gsort if a.target=="GNU" else (a.sort or ROOT/"bin/sort-darwin-arm64")
    uniq=guniq if a.target=="GNU" else ROOT/"bin/uniq-darwin-arm64"
    profiles=[(65536,128,32)]
    if a.large:profiles.append((1<<20,1024,2048))
    if a.mode=="mesures":profiles=[(2048,128,32),(65536,128,32),(131072,128,1024),(131072,128,3),(128,1<<20,4)]+([(1<<20,1024,2048)] if a.large else [])
    results=[]
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as d:
        root=Path(d);tmp=root/"runs";tmp.mkdir()
        for n,width,count in profiles:
            size=n*width;free=shutil.disk_usage(root).free
            if free<4*size+(2<<30):raise OSError("espace de travail insuffisant")
            keys,freq,order=corpus(n,width,count)
            expect=b"".join(f"{v:7d} ".encode()+k for k,v in zip(keys,freq))
            output=root/"result";source=root/"gros-fichier.txt"
            if a.mode=="pipeline":
                with source.open("wb") as f:
                    for i in order:f.write(keys[i])
                oracle=root/"gnu"
                g=measure([[gsort,source],[guniq,"-c"]],source,oracle,tmp,n)
                m=measure([[sort,source],[uniq,"-c"]],source,output,tmp,n)
                ok=oracle.read_bytes()==output.read_bytes()==expect
                measurements=[dict(profile="GNU",**g),dict(profile=a.target,**m)]
                ok &= all(r["status"]==0 for item in measurements for r in item["processes"])
            else:
                with source.open("wb") as f:
                    for k,v in zip(keys,freq):
                        for _ in range(v):f.write(k)
                ok=True;measurements=[]
                for opt in ["-c","-D","--group"]:
                    m=measure([[uniq,opt,source]],source,output,tmp,n)
                    oracle=root/"gnu"
                    with oracle.open("wb") as out:subprocess.run([guniq,opt,source],stdout=out,check=True,env={**os.environ,"LC_ALL":"C"})
                    ok &= hashfile(oracle)==hashfile(output) and m["processes"][0]["status"]==0
                    if opt=="-c":ok &= output.read_bytes()==expect
                    measurements.append(dict(option=opt,output_sha256=hashfile(output),**m))
            result=dict(seed=SEED,rows=n,width=width,keys=count,bytes=size,free_before=free,input_sha256=hashfile(source),
                expected_count_sha256=hashlib.sha256(expect).hexdigest(),first_count=freq[0],last_count=freq[-1],
                count_sum=sum(freq),uniq_sha256=hashfile(uniq),sort_sha256=hashfile(sort) if a.mode=="pipeline" else None,
                ok=ok,measurements=measurements)
            results.append(result)
            if a.output:a.output.write_text(json.dumps(results,indent=2)+"\n")
            print(size,"octets",n,"lignes",count,"clés",ok,flush=True)
            if not ok:raise SystemExit(1)
if __name__=="__main__":main()
