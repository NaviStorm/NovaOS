"""Mesures Darwin du processus sed et essais sur fichiers fictifs."""
from pathlib import Path
import hashlib
import json
import os
import platform
import re
import subprocess
import sys
import tempfile
import time

def sha(file):
    with file.open("rb") as f:return hashlib.file_digest(f,"sha256").hexdigest()

def run(binary,*args,**kw):return subprocess.run([binary,*args],capture_output=True,timeout=40,**kw)

def status(result):
    return {"code":result.returncode,"stderr":result.stderr.decode(errors="replace")}

def measurements(binary,wd):
    rows=[]
    for kind,mib in [("short",1),("short",8),("short",32),("long",1),("long",8),("hold",1),("hold",8)]:
        line=b"a"+b"x"*62+b"\n"
        path=wd/"input"
        if kind=="long": path.write_bytes(b"a"*(mib*1024**2-1)+b"\n")
        else:path.write_bytes(line*(mib*1024**2//len(line)))
        script="s/^a/Z/" if kind!="hold" else "1h;1!H;$ {g;p;}"
        args=["-n"] if kind=="hold" else []
        args += [script,str(path)]
        with (wd/"output").open("wb") as out:
            p=subprocess.run(["/usr/bin/time","-l",binary,*args],stdout=out,stderr=subprocess.PIPE,timeout=120)
        p.check_returncode()
        if kind=="hold": expected=sha(path)
        else:
            data=path.read_bytes();expected=hashlib.sha256(data.replace(b"a",b"Z",1) if kind=="long" else data.replace(b"a",b"Z")).hexdigest()
        actual=sha(wd/"output");assert actual==expected
        report=p.stderr.decode()
        rows.append(dict(kind=kind,bytes=path.stat().st_size,args=args[:-1],output_sha256=actual,
            peak_rss_bytes=int(re.search(r"(\d+)\s+maximum resident set size",report)[1]),time=report))
    return rows

def failures(binary,probe,wd):
    rows={}
    src=wd/"source";backup=wd/"source.bak"
    def inspect():
        return {f.name:dict(bytes=f.stat().st_size,sha256=sha(f),mode=oct(f.stat().st_mode&0o777)) for f in wd.iterdir() if f.is_file()}
    src.write_bytes(b"original\n");src.chmod(0o444)
    for name,exe in [("readonly-GNU","/opt/homebrew/bin/gsed"),("readonly-Nova",binary)]:
        src.chmod(0o644);src.write_bytes(b"original\n");src.chmod(0o444)
        if backup.exists():backup.unlink()
        p=run(exe,"-i.bak","s/original/changed/",src)
        rows[name]=dict(**status(p),files=inspect())
    assert p.returncode!=0 and src.read_bytes()==b"original\n" and not backup.exists()
    src.chmod(0o640)
    subprocess.run(["chmod","+a","everyone deny write",src],check=True)
    p=run(binary,"-i.bak","s/original/changed/",src)
    assert p.returncode==4 and src.read_bytes()==b"original\n" and not backup.exists()
    rows["acl-deny-write"]=dict(**status(p),files=inspect())
    subprocess.run(["chmod","-N",src],check=True)
    subprocess.run(["xattr","-w","org.nova.sed-test","fixture",src],check=True)
    subprocess.run(["chmod","+a","everyone deny execute",src],check=True)
    p=run(binary,"-i.bak","s/original/changed/",src);p.check_returncode()
    acl=subprocess.check_output(["ls","-le",src],text=True)
    xattr=subprocess.check_output(["xattr","-p","org.nova.sed-test",src],text=True).strip()
    assert "deny execute" in acl and xattr=="fixture" and backup.read_bytes()==b"original\n"
    rows["metadata"]=dict(acl=acl,xattr=xattr,files=inspect())
    src.write_bytes(b"original\n");backup.unlink()
    p=run(probe,"-i.bak","s/original/changed/",src,env={**os.environ,"NOVA_SED_TEST_EIO":"1"})
    assert p.returncode==4 and src.read_bytes()==b"original\n" and not backup.exists()
    rows["EIO-injected-before-sync"]=dict(**status(p),files=inspect())
    with subprocess.Popen([probe,"-i.bak","s/original/changed/",src],
            env={**os.environ,"NOVA_SED_TEST_PAUSE_PUBLICATION":"1"}) as process:
        until=time.monotonic()+10
        while time.monotonic()<until:
            pid,state=os.waitpid(process.pid,os.WUNTRACED|os.WNOHANG)
            if pid:
                assert os.WIFSTOPPED(state);break
            time.sleep(.02)
        else:
            process.kill();raise AssertionError("point de publication non atteint")
        process.kill();process.wait()
    rows["SIGKILL-after-backup-before-rename"]=inspect()
    assert src.read_bytes()==backup.read_bytes()==b"original\n"
    p=run(probe,"-n","H;$ {g;p;}",input=b"abc\n"*100000,
          env={**os.environ,"NOVA_SED_TEST_MAX_BLOCK":"65536"})
    assert p.returncode==4 and not p.stdout and b"memory" in p.stderr,(p.returncode,p.stderr)
    rows["hold-allocation"]=dict(**status(p),input_bytes=400000,block_limit=65536,stdout_bytes=len(p.stdout))
    return rows

def no_space(binary,wd):
    image=wd/"tiny.dmg";mount=wd/"mounted";mount.mkdir()
    subprocess.run(["hdiutil","create","-size","16m","-fs","HFS+","-volname","NovaSedTest",image],check=True,capture_output=True)
    subprocess.run(["hdiutil","attach","-nobrowse","-mountpoint",mount,image],check=True,capture_output=True)
    try:
        src=mount/"input";src.write_bytes((b"a"*4095+b"\n")*512);initial=sha(src)
        free=os.statvfs(mount);filler=mount/"filler"
        with filler.open("wb") as f:
            f.write(b"x"*max(0,(free.f_bavail*free.f_frsize)-(256*1024)))
            f.flush();os.fsync(f.fileno())
        p=run(binary,"-i.bak","s/^a/z/",src)
        assert p.returncode!=0 and sha(src)==initial and not (mount/"input.bak").exists()
        return dict(**status(p),original=initial,after=sha(src),directory=sorted(x.name for x in mount.iterdir()))
    finally:
        subprocess.run(["hdiutil","detach",mount],check=True,capture_output=True)

def main():
    binary,probe,output=map(Path,sys.argv[1:4])
    state={"platform":platform.platform(),"machine":platform.machine(),"binary":sha(binary),"probe":sha(probe)}
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as tmp:
        wd=Path(tmp)
        for name in ["memory","failures","volume"]: (wd/name).mkdir()
        state["memory"]=measurements(binary,wd/"memory")
        state["faults"]=failures(binary,probe,wd/"failures")
        try:state["full_disk"]=no_space(binary,wd/"volume")
        except subprocess.CalledProcessError as e:state["full_disk"]={"blocked":str(e),"stderr":e.stderr.decode(errors="replace") if e.stderr else ""}
    output.write_text(json.dumps(state,ensure_ascii=False,indent=2)+"\n")
if __name__=="__main__":main()
