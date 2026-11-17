"""Oracles du catalogue SDK pour les vecteurs indépendants de la date du jour."""
import base64
import json
from options import ENV, GNU, NOVA, run

def rows(path):
    for line in path.read_text().splitlines():
        r=json.loads(line)
        if r["kind"]!="diff":continue
        args=r["argv"]
        if r["TZ"]!="UTC0":
            if len(args)==3 and args[1]=="-d" and args[2].startswith("@"):
                code,so,se=run(GNU,args,ENV,None)
                yield "UTC-"+str(r["id"]),list(map(str.encode,args)),6,(code,base64.b64decode(so),base64.b64decode(se)),"GNU, TZ=UTC0"
            continue
        code,so,se,effect=r["gnu"]
        out,err=base64.b64decode(so),base64.b64decode(se)
        mode="GNU C"
        if args in ([],["--"],["+%s"],["--debug"],["-u"],["--uct"],["--utc"],["--universal"],["-I"],["-R"],["--rfc-email"],["--rfc-822"],["--rfc-2822"],["--rfc-3339=ns"]) or (len(args)==3 and args[0]=="-d" and args[1] in ("now","today","yesterday","tomorrow","last Monday","next Fri","1 hour ago")):
            yield r["id"],list(map(str.encode,args)),6,(0,b"",b""),"GNU sur intervalle RTC"
            continue
        if effect:
            code,err,mode=1,b"date: cannot set date: Operation not permitted\n","refus réglage v1"
            if args==["02291234"]:continue
        elif "--resolution" in args and code==0:
            out=b"1/000000000\n" if args==["--resolution","+%s/%N"] else b"1.000000000\n"
            mode="résolution RTC"
        elif any(v in ("batch","ref","missing","denied",".") or v.startswith("--reference=") or v=="--file=-" for v in args) and not (b"exclusive" in err or b"may not" in err or b"ambiguous" in err):
            continue
        elif code==0 and not any(v.startswith(("@","--date=@","-ud@","20","1999-","10000-","2147483647-")) for v in args):
            continue
        if any(b<32 and b!=10 for b in out):
            code,out,err,mode=1,b"","date: argument invalide\n".encode(),"port terminal v1"
        elif any("%-N" in v for v in args):
            out=out.replace(b"123456 ",b"1 ").replace(b"123456\n",b"1\n").replace(b"765432 765432 ",b"765432 7 ")
            mode="résolution RTC"
        yield r["id"],list(map(str.encode,args)),6,(code,out,err),mode
    for flag in ("-r","-f"):
        yield flag,[flag.encode(),b"absent"],6,(1,b"","date: absent: accès refusé\n".encode()),"absence de délégation"
    yield "stdin",[b"-f",b"-"],6,(1,b"","date: standard input: accès refusé\n".encode()),"entrée absente"
    for flag in ("--help","--version"):
        for tag in ("fr","en"):
            args=[flag] if tag=="en" else [flag,"--","nsh-doc=fr"]
            code,out,err=run(NOVA,[flag],dict(ENV,LC_ALL=tag),None)
            yield flag+tag,list(map(str.encode,args)),6,(code,base64.b64decode(out),base64.b64decode(err)),"ARC-026"
    yield "RTC",[b"+%s"],6,(0,b"",b""),"intervalle RTC"
