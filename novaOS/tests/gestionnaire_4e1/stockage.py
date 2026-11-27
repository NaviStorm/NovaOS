"""Relecture indépendante du volume de 512 blocs de la fixture ARM du pilote VirtIO."""
import hashlib
import re
import subprocess

def verifier_volume(lecteur,contenu):
    if len(contenu)!=512*4096:
        raise ValueError("Géométrie P9 de la fixture")
    def lire(commande,*arguments):
        return subprocess.run([lecteur,commande,"512","512",*arguments],input=contenu,
                              capture_output=True,check=True).stdout
    tetes=re.findall(r"slot=([01]) génération=(\d+) blocs=512 FS=(\w+) LV=(\w+) manifeste=(\d+) sha256=([0-9a-f]{64})",lire("inspect").decode())
    if len(tetes)!=2 or {int(v[0]) for v in tetes}!={0,1} or len({v[2:4] for v in tetes})!=1:
        raise ValueError("Deux ancres P9 cohérentes attendues")
    slot,_,fs,lv,_,_=max(tetes,key=lambda h:int(h[1]))
    lire("verify",fs,lv)
    fichiers={}
    for chemin,attendu in (("/source",b"hote-Nova!"),("/sortie",b"ancien")):
        data=lire("salvage",fs,lv,slot,chemin)
        if data!=attendu: raise ValueError("Fichier témoin P9 altéré "+chemin)
        fichiers[chemin]=hashlib.sha256(data).hexdigest()
    return dict(ancres=tetes,verification=True,fichiers=fichiers,
                lecteur_sha256=hashlib.sha256(lecteur.read_bytes()).hexdigest())
