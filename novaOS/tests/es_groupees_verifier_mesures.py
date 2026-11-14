"""Recalculer les percentiles et vérifier les compteurs de la qualification E/S."""
import argparse
from collections import defaultdict
import json
import math
from pathlib import Path


def cellule(ligne):
    return tuple(ligne[k] for k in ("environnement", "architecture", "secteur", "cas"))


def verifier_percentiles(mesures, percentiles):
    groupes = defaultdict(list)
    essais = set()
    for m in mesures:
        temps = m["commande_succes_ms"]
        if m["essai"] in essais or not math.isfinite(temps) or temps <= 0:
            raise ValueError(f"Mesure invalide ou en double {m['essai']}")
        essais.add(m["essai"])
        groupes[cellule(m)].append(temps)

    annonces = {cellule(m): m for m in percentiles}
    if not groupes or annonces.keys() != groupes.keys() or len(annonces) != len(percentiles):
        raise ValueError("Les cellules des percentiles ne correspondent pas aux observations")
    lents = []
    for cle, temps in sorted(groupes.items()):
        temps.sort()
        n = len(temps)
        chiffres = {f"p{p}": temps[(p * n + 99) // 100 - 1] for p in (50, 95, 99)}
        chiffres.update(n=n, max=temps[-1], depassements=sum(t >= 1000 for t in temps))
        if any(annonces[cle][k] != v for k, v in chiffres.items()):
            raise ValueError(f"Percentile, maximum ou décompte incorrect pour {cle}")
        print(*cle, *(chiffres[k] for k in ("n", "p50", "p95", "p99", "max", "depassements")), sep="\t")
        if chiffres["depassements"]:
            lents.append(cle)
    return lents


def verifier_sonde(s):
    c = s["compteur"]
    pret = c["appels_locaux_pret"]
    total = sum(c["ipc_par_liaison"].values()) + c["dons_acquittes"]
    avec_copies = total + sum(c["controles_noyau_locaux"].values()) + sum(pret)
    if not (len(pret) == 6 and total == c["total"] and avec_copies == c["avec_copies_locales_pret"]
            and 0 < avec_copies - pret[2] == c["total_avec_controles_locaux"] <= 64
            and c["bloc"]["flush"]["nombre"] == 2
            and 0 < c["persistance_acquise_ms"] <= c["appel_publish_ms"]):
        raise ValueError(f"Contrat IPC ou F2 incohérent pour {s['essai']}")
    for cle in ("backend_hmp", "backend_qmp"):
        if s.get(cle) is not None:
            for op, bloc in (("rd", "lecture"), ("wr", "ecriture"), ("flush", "flush")):
                if s[cle][op + "_operations"] != c["bloc"][bloc]["nombre"]:
                    raise ValueError(f"Nombre d’opérations différent pour {s['essai']} {op}")


def verifier_relectures(rapport):
    empreintes = {}
    for m in rapport["mesures_individuelles"]:
        if "sha256" in m and empreintes.setdefault(m["cas"], m["sha256"]) != m["sha256"]:
            raise ValueError(f"Contenu différent entre sauvegardes du scénario {m['cas']}")
    mesures_utm = [m for m in rapport["mesures_individuelles"] + rapport["sondes"]
                   if m["environnement"].startswith("Application UTM ")]
    for m in mesures_utm:
        relu = m["relecture"]
        if (not m["octets_relus"] or relu["generation"] != max(m["generations"])
                or relu["source_sha256"] != rapport["source_dense_sha256"] or relu["octets_destination"] <= 0
                or relu["destination_sha256"] != empreintes[m["cas"]]):
            raise ValueError(f"Relecture indépendante incohérente pour {m['essai']}")


def verifier(rapport):
    lents = verifier_percentiles(rapport["mesures_individuelles"], rapport["percentiles_par_cellule"])
    verifier_relectures(rapport)
    politique = rapport.get("temoin_politique_utm", [])
    hors_sonde = [s for s in politique if not s["sonde"]]
    if hors_sonde:
        verifier_percentiles(hors_sonde, rapport["percentiles_temoin_politique_utm"])
    for s in rapport["sondes"] + rapport["temoins_utm_instrumentes_alternes"]:
        verifier_sonde(s)
    for s in politique:
        if s["sonde"]:
            verifier_sonde(s)
    return lents


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("rapport", type=Path)
    p.add_argument("--exiger-seuil", action="store_true",
                   help="retourner une erreur dès qu’une sauvegarde hors sonde atteint une seconde")
    args = p.parse_args()
    print("environnement\tarchitecture\tsecteur\tscénario\tn\tp50_ms\tp95_ms\tp99_ms\tmaximum_ms\tdépassements")
    lents = verifier(json.loads(args.rapport.read_text()))
    raise SystemExit(int(args.exiger_seuil and bool(lents)))
