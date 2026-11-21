"""Décisions natives bornées ; aucun succès déduit d’un timeout."""
import re

OLD=re.compile(rb'NXBAIL op=NSHOPEN1 tag=(\d+) decision=(\d+) valide=(\d+) echeance=(\d+) retour=(\d+) fin=(\d+)\r?\n')
NEW=re.compile(rb'NXAUTH ([0-9a-f]{64}) ((?:[0-9a-f]{16} ){8})\r?\n')
def decisions(raw):
    result=[]
    for m in NEW.finditer(raw):
        v=[int(x,16) for x in m[2].split()]
        result.append(dict(defi=m[1].decode(),tag=v[0],emission=v[1],echeance=v[2],signature=v[3],verification=v[4],retour=v[5],bail=v[6],fin=v[7]))
    if result:return result
    return [dict(tag=int(m[1]),verification=int(m[2]),retour=int(m[5]),bail=int(m[4]),fin=int(m[6]),expire=b'NXLOGIN refus defi expire' in raw) for m in OLD.finditer(raw)]

