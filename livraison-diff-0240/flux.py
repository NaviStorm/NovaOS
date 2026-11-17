"""Vrais tubes Darwin, dont fermeture du lecteur et disposition SIGPIPE héritée."""
from laboratoire import *
def closed(binary, default):
    r,w=os.pipe();os.close(r)
    try:
        p=subprocess.run(["diff","--help"],executable=binary,env=ENV,stdout=w,stderr=subprocess.PIPE,restore_signals=default,timeout=5)
        return [p.returncode,"",p.stderr.hex()]
    finally:os.close(w)
def through(binary,d):
    with subprocess.Popen(["cat",d/"left"],stdout=subprocess.PIPE,env=ENV) as producer:
        with subprocess.Popen(["diff","-","right"],executable=binary,cwd=d,env=ENV,stdin=producer.stdout,stdout=subprocess.PIPE,stderr=subprocess.PIPE) as comp:
            producer.stdout.close()
            with subprocess.Popen(["cat"],stdin=comp.stdout,stdout=subprocess.PIPE,env=ENV) as consumer:
                comp.stdout.close();output=consumer.communicate(timeout=10)[0]
            err=comp.stderr.read()
            assert producer.wait()==consumer.returncode==0
            return [comp.wait(),output.hex(),err.hex()]
with tempfile.TemporaryDirectory(dir=TMP,prefix="diff-flux-") as td:
    d=Path(td);(d/"left").write_bytes(b"ligne\n");rows=[]
    for title,action in [("lecteur fermé, SIGPIPE par défaut",lambda b:closed(b,True)),("lecteur fermé, SIGPIPE ignoré",lambda b:closed(b,False))]:
        g=action(oracle());n=action(NOVA);rows.append(dict(test=title,GNU=g,Nova=n,ok=g==n))
    for name,data in [("identiques",b"ligne\n"),("différents",b"fin\n")]:
        (d/"right").write_bytes(data);g=through(oracle(),d);n=through(NOVA,d);rows.append(dict(test=name,GNU=g,Nova=n,ok=g==n))
    (d/"right").unlink();g=through(oracle(),d);n=through(NOVA,d);rows.append(dict(test="opérande absent",GNU=g,Nova=n,ok=g==n))
    save(HERE/"preuves/flux-darwin.json",rows)
    assert all(r["ok"] for r in rows)
