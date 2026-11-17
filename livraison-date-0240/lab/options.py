"""Matrice différentielle indépendante des deux exécutables, flux en base64."""
import argparse
import base64
import itertools
import json
import os
from pathlib import Path
import subprocess as sp
import tempfile
import time

HERE = Path(__file__).resolve().parents[1]
GNU = Path("/opt/homebrew/opt/coreutils/libexec/gnubin/date")
NOVA = HERE / "bin/date"
TMP = Path(os.environ["NOVAOS_TMPDIR"])
FIXED = 1709164800
ENV = dict(os.environ, LC_ALL="C", TZ="UTC0")
def encoded(data):
    return base64.b64encode(data).decode()

def cases(full):
    # La référence est nommée date via son lien Homebrew afin de comparer stderr brut.
    for a in ([], ["--"], ["+%s"], ["--debug"], ["-u"], ["--uct"], ["--utc"], ["--universal"],
              ["-I"], ["-R"], ["--rfc-email"], ["--rfc-822"], ["--rfc-2822"], ["--resolution"],
              ["--resolution","+%s/%N"], ["--rfc-3339=ns"], ["--debug","--resolution"],
              ["-d","@0"], ["--date=@-0.123456789"], ["-f","batch"], ["--file=-"],
              ["-r","ref"], ["--reference=ref"], ["-s","@0"], ["--set=@-1.25"], ["022912342024.56"],
              ["-ud@0","--","+%F"], ["--","--help"], ["-d","--help"], ["-s","broken"],
              ["-f","missing"], ["-r","missing"], ["-f","denied"], ["-f","."],
              ["--unknown"], ["--help=x"], ["--rf"], ["--d"], ["-X"], ["-d"], ["-f"], ["-r"], ["-s"],
              ["+%Y","+%m"], ["-R","+%s"], ["-d","@0","word"], ["--debug","-d","@1","-d","@2"],
              ["--debug","-d","2024-02-29"], ["--debug","-d","bogus"], ["--debug","-f","batch"],
              ["-u","--","022912342024.56"], ["-d","@0","-I","-R"], ["-R","-Iseconds","-d","@0"]):
        yield a,"UTC0"
    for flag in ("-I","--iso-8601=","--rfc-3339="):
        for val in ["date","hours","minutes","seconds","ns","s","auto","bad",""]:
            yield [flag+val,"-d","@-0.123456789"],"Europe/Paris"
    for left,right in itertools.combinations([["-d","@0"],["-f","batch"],["-r","ref"],["--resolution"],["-s","@1"]],2):
        yield left+right,"UTC0"
    for tag in ("UTC0","Europe/Paris","America/New_York","Asia/Kathmandu"):
        for date in ["@0","@-0.1","2024-02-29","2023-02-29","2024-03-31 02:30:00","2024-11-03 01:30:00",
                     'TZ="Europe/Paris" 2024-03-31 02:30:00',"last Monday","2000-02-29 1 year","1970-01-01 UTC +90 minutes"]:
            yield ["-d",date,"+%s.%N %F %T %Z %:z"],tag
    for fmt in ["%% %n%t", "%C %y %Y %g %G %a %A %b %h %B %m %d %e %D %F %x %c",
                "%H %I %k %l %M %S %N %s %p %P %r %R %T %X","%j %q %u %w %U %V %W",
                "%z %:z %::z %:::z %Z","%3N %6N %-N %_N %12N","%Od %EY %^B %#p %_4m %+6Y"]:
        yield ["-d","@-1.234567890","+"+fmt],"UTC0"
    if not full: return
    for k in "aAbBhCcDdeFgGHIjklmMnNpPqrRsStTuUVwWxXyYzZ%":
        for mod in ["","_","-","0","^","#","6","+6","E","O"]:
            yield ["-d","@1.123456789","+"+"%"+mod+k],"UTC0"
    for name,tail in [("date",[" @0"]),("debug",[]),("file",[" batch"]),("iso-8601",[]),("reference",[" ref"]),
                      ("resolution",[]),("rfc-email",[]),("rfc-3339",["=ns"]),("set",[" @1"]),("universal",[]),("uct",[]),("utc",[])]:
        for n in range(1,len(name)+1):
            yield ("--"+name[:n]+"".join(tail)).split(),"UTC0"
    for code in ["02291234","0229123424.56","0229123468.59","0229123469","023012342024","022912342024.99","--bad"]:
        yield [code],"UTC0"
    for text in ["","now","today","yesterday","tomorrow","1 hour ago","next Fri","Mon, 2024-06-15","Feb 29 2024",
                 "1999-12-31T23:59:59-05:00","@-1.0000000001","10000-01-01","2147483647-01-01","bad-date"]:
        yield ["-d",text,"+%s.%N %F %T"],"UTC0"

def run(binary,args,env,directory):
    result=sp.run(["date",*args],executable=binary,env=env,cwd=directory,input=b"@0\nbogus\n@1.25\n",capture_output=True,timeout=8)
    return [result.returncode,encoded(result.stdout),encoded(result.stderr)]

def main():
    p=argparse.ArgumentParser(description="Compare options, effets et flux bruts de GNU et Nova sous Darwin.")
    modes=p.add_mutually_exclusive_group(required=True)
    modes.add_argument("--rapide",action="store_true");modes.add_argument("--complet",action="store_true")
    p.add_argument("--out",type=Path,default=TMP/"date-arc027/options.jsonl")
    a=p.parse_args();a.out.parent.mkdir(parents=True,exist_ok=True)
    failures=0;count=0
    with tempfile.TemporaryDirectory(dir=TMP,prefix="date-options-") as d,a.out.open("w") as f:
        d=Path(d);lib=d/"horloge.dylib";record=d/"effet"
        sp.run(["cc","-arch","arm64","-dynamiclib",HERE/"lab/horloge.c","-o",lib],check=True)
        env=dict(ENV,DYLD_INSERT_LIBRARIES=str(lib),DATE_TEST_RECORD=str(record))
        if os.geteuid()==0:raise RuntimeError("banc de réglage réservé à un processus non privilégié")
        for binary in (GNU,NOVA):
            assert run(binary,["+%s"],env,d)==[0,encoded(f"{FIXED}\n".encode()),""],"interposition inactive"
        (d/"batch").write_bytes(b"@0\nbogus\n@-0.25");(d/"ref").touch()
        os.utime(d/"ref",ns=(1234567890,1234567890))
        denied=d/"denied";denied.touch();denied.chmod(0)
        def emit(row):
            nonlocal count,failures
            count+=1;failures+=int(not row["ok"]);row["id"]=count
            print(json.dumps(row,ensure_ascii=False),file=f)
        for args,tz in cases(a.complet):
            outputs=[]
            for binary in (GNU,NOVA):
                record.write_bytes(b"")
                outputs.append([*run(binary,args,dict(env,TZ=tz),d),encoded(record.read_bytes())])
            emit(dict(kind="diff",argv=args,TZ=tz,gnu=outputs[0],nova=outputs[1],ok=outputs[0]==outputs[1]))
        for alias in ("-u","--utc","--uct","--universal","-I","--iso-8601","-R","--rfc-email","--rfc-822","--rfc-2822","--rfc-3339=ns","--debug"):
            for value in ("@0","@-0.5"):
                args=[alias,"-d",value]
                outputs=[[*run(b,args,env,d),""] for b in (GNU,NOVA)]
                emit(dict(kind="diff",argv=args,TZ="UTC0",gnu=outputs[0],nova=outputs[1],ok=outputs[0]==outputs[1]))
        for locale in ("fr_FR.UTF-8","en_US.UTF-8","am_ET.UTF-8","fa_IR.UTF-8"):
            for fmt in ("+%A %B","+%Y-%m-%d","+%c","+%x %X"):
                args=["-ud","@0",fmt]
                outputs=[run(b,args,dict(env,LC_ALL=locale,TZ="Europe/Paris"),d) for b in (GNU,NOVA)]
                emit(dict(kind="locale",locale=locale,argv=args,gnu=outputs[0],nova=outputs[1],ok=outputs[0]==outputs[1]))
        for args in [["-s","@0"],["--set=@1.25"],["022912342024.59"]]:
            outputs=[run(b,args,dict(env,DATE_TEST_DENY="1"),d) for b in (GNU,NOVA)]
            emit(dict(kind="refus-réglage",argv=args,gnu=outputs[0],nova=outputs[1],ok=outputs[0]==outputs[1]))
        for b in (GNU,NOVA):
            start=time.time_ns();raw=run(b,["+%s%N"],ENV,d);end=time.time_ns()
            val=int(base64.b64decode(raw[1]));emit(dict(kind="horloge-réelle",outil=str(b),avant=start,apres=end,valeur=val,
                ok=raw[0]==0 and start-1000<=val<=end))
        for flag in ("--help","--version"):
            for tag in ("fr","fr-CA","fr--CA","fr-QQ","en-US"):
                got=run(NOVA,[flag],dict(env,LC_ALL=tag),d)
                if flag=="--help":ok=("Afficher" if tag in ("fr","fr-CA","fr-QQ") else "Display").encode() in base64.b64decode(got[1])
                else:ok=b"nova-cli" in base64.b64decode(got[1])
                emit(dict(kind="ARC-026",argv=[flag],tag=tag,nova=got,ok=got[0]==0 and not got[2] and ok))
    print(f"{count-failures}/{count} conformes, {failures} écarts. {a.out}")
    return int(bool(failures))
if __name__=="__main__":raise SystemExit(main())
