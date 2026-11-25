"""Relecture hors invité du volume et calcul sur les complétions FLUSH physiques."""
import ast
import json
import math
import re
from types import SimpleNamespace

from fichiers import oracle
from virtio_net_confinement import digest
from virtio_net_domaines_arm import verifier


def verifier_charge(out, extrait, initial, secteur, trafic):
    serie=(out/"serie.log").read_text()
    m=re.search(r"NET P9 resultat=true hz=(\d+) cycles=(\d+) rx_debut=(\d+) rx_fin=(\d+) octets=(\[[\d, ]+\]) temps=(\[[\d, ]+\]) flush=(\d+)",serie)
    if (not m or "NET P9 256 transactions WRITE SYNC READ octets conformes" not in serie or
            f"NET P9 coexistence trafic={str(trafic).lower()} cadres_fin=0" not in serie):
        raise ValueError("Transactions, progression ou restitution non démontrées")
    hz,cycles,debut,fin=map(int,m.groups()[:4])
    if not hz or not cycles or (trafic and not debut<fin<999984) or (not trafic and (debut or fin)):
        raise ValueError("La progression du trafic ne correspond pas au profil")
    octets=ast.literal_eval(m[5])
    temps=ast.literal_eval(m[6])
    flush=[n for match in re.findall(r"NET P9 FLUSH cycles=(\[[\d, ]+\])",serie) for n in ast.literal_eval(match)]
    if len(flush)!=int(m[7]) or not 256<=len(flush)<4096 or min(flush)<=0 or len(temps)!=3 or min(temps)<=0:
        raise ValueError("Répartition des échantillons FLUSH invalide")
    if len(octets)!=2 or min(octets)<256*512:
        raise ValueError("Volume transféré insuffisant")
    obj=SimpleNamespace(image=extrait,sector=secteur)
    generation=oracle(obj,bytes(((i*73)&255)^255 for i in range(512)))
    depart=oracle(SimpleNamespace(image=initial,sector=secteur),b"ancien")
    recu=re.search(r"NET P9 generations ([0-9a-f]{16}) ([0-9a-f]{16}) ([0-9a-f]{16})",serie)
    if not recu:
        raise ValueError("Générations de l’application absentes")
    avant,apres,lectures=(int(s,16) for s in recu.groups())
    if (avant,apres)!=(depart,generation) or generation!=depart+256+lectures:
        raise ValueError("Les publications et les mises à jour après lecture ne couvrent pas la tête relue")
    domaines=verifier(out,True,ethernet=trafic)
    if trafic:
        pair=json.loads((out/"pair.json").read_text())
        if (not pair["succes"] or pair["trames_tx"]!=1000000 or pair["reponses_envoyees"]!=1000000
                or "NET L2 total TX=1000000 RX=1000000 index=16960 rearmements=62499" not in serie):
            raise ValueError("Preuve du trafic soutenu incomplète")
    tries=sorted(flush)
    return dict(succes=True,trafic=trafic,domaines=domaines,volume_sha256_final=digest(extrait),
                generation_initiale=depart,generation_finale=generation,publications_apres_lecture=lectures,rx_pendant_p9=fin-debut,
                cycles_par_flush=flush,hz=hz,n_flush=len(flush),octets_physiques=octets,cycles_stockage=cycles,
                debit_Mio_s_sur_parcours=[n*hz/cycles/2**20 for n in octets],
                debit_Mio_s_sur_service=[n*hz/t/2**20 for n,t in zip(octets,temps[:2])],
                flush_us={str(q):tries[math.ceil(len(tries)*q/100)-1]*1e6/hz for q in (50,95,99,100)})
