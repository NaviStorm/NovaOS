"""Témoin indicatif Darwin : processus neuf, PTY 24×80, premier texte visible de Vim."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import struct
import subprocess
import tempfile
import termios
import time

def measure(path):
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH",24,80,0,0))
    try:
        begin = time.monotonic_ns()
        process = subprocess.Popen(["/usr/bin/vim","-Nu","NONE","-i","NONE","-n","-N",
                                    "--cmd","set nowrap t_RV= t_u7=",str(path)],stdin=slave,stdout=slave,
                                   stderr=slave,env={**os.environ,"TERM":"xterm"})
        try:
            data=b""
            while b"DEBUT-NOVA-VI" not in data:
                if not select.select([master],[],[],45)[0]:
                    raise RuntimeError("première ligne absente")
                data+=os.read(master,32768)
            elapsed=(time.monotonic_ns()-begin)/1e6
            os.write(master,b":q!\r")
            deadline=time.monotonic()+15
            while process.poll() is None and time.monotonic()<deadline:
                if select.select([master],[],[],0.1)[0]:
                    os.read(master,32768)
            assert process.wait(timeout=1)==0
            return round(elapsed,3)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
    finally:
        os.close(master)
        os.close(slave)

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("rapport",type=Path)
    args=p.parse_args()
    results=[]
    with tempfile.TemporaryDirectory(prefix="nova-vi-macos-") as d:
        path=Path(d)/"source.txt"
        line=b"DEBUT-NOVA-VI".ljust(127,b"A")+b"\n"
        block=line*8192
        for size in [100_000_000,100<<20]:
            with path.open("wb") as f:
                whole,tail=divmod(size,len(block))
                for _ in range(whole):f.write(block)
                f.write(block[:tail])
                f.flush()
                os.fsync(f.fileno())
            results.append(dict(octets=size,premier_ecran_ms=measure(path)))
    args.rapport.write_text(json.dumps(dict(version=subprocess.check_output(
        ["/usr/bin/vim","--version"],text=True).splitlines()[0],donnees_denses=True,
        cache="après création, non purgé",resultats=results),indent=2,ensure_ascii=False)+"\n")
