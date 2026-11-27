"""Recalculer le bilan 4D à partir des séries QEMU et des observations indépendantes du banc."""
import argparse
import json
import math
from pathlib import Path
import re
import statistics

from qualifier import CASES, NEG, STATIQUE

TEMOIN = "temoin-3e"
ENDURANCE = "cycles-256"
PERF = ("echo1514-reset", TEMOIN, STATIQUE)
PHASES = {
    "decouverte_pci": r"D4 decouverte ticks=(\d+)",
    "crypto_et_admission": r"crypto_ticks=(\d+)",
    "inventaire_el0": r"inventaire_ticks=(\d+)",
    "selection_et_ipc": r"resolution_ipc_ticks=(\d+)",
    "creation_scellee": r"D4 lancement scelle [^\n]+? ticks=(\d+)",
    "activation_vers_open": r"D4 activation publication_open_ticks=(\d+)",
    "fermeture_vers_restitution": r"D4 fermeture restitution_ticks=(\d+)",
    "retrait_superviseur": r"D4 duree phase=retrait ticks=(\d+)",
    "reconciliation": r"D4 duree phase=reconciliation ticks=(\d+)",
}
BILAN = r"D4 cycle avant=(\d+) minimum=(\d+) apres=(\d+) pic=(\d+) ipc=(\d+) dma=(\d+)"


def exiger(ok, message):
    if not ok:
        raise ValueError(message)


def entiers(ligne):
    return {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", ligne)}


def ligne(serie, prefixe):
    return next(v for v in serie.splitlines() if v.startswith(prefixe))


def quantiles(valeurs):
    valeurs = sorted(valeurs)
    return dict(n=len(valeurs), **{
        f"p{q}": round(valeurs[math.ceil(q * len(valeurs) / 100) - 1], 6)
        for q in (50, 95, 99)
    }, maximum=round(valeurs[-1], 6))


def profil_perf(serie, preuve):
    bilan = entiers(ligne(serie, "ETH3 bilan "))
    hz = bilan["hz"]
    lots = {}
    for b, n, total, qs in re.findall(r"ETH3 lot=(\d+) n=(\d+) somme_ticks=(\d+) quantiles_ticks=\[([^]]+)\]", serie):
        n, total = int(n), int(total)
        exiger(n == 1024 and b in ("1", "4", "8"), "série Ethernet inattendue")
        quant = [int(v) / hz * 1000 for v in qs.split(",")]
        lots[b] = dict(n=n, debit_Mbit_s=n * int(b) * 1514 * 8 * hz / total / 1e6,
                       p50_ms=quant[0], p95_ms=quant[1], p99_ms=quant[2], p999_ms=quant[3])
    exiger(len(lots) == 3 and preuve["trames_par_sens"] == 13312, "profil Ethernet incomplet")
    acteurs = ligne(serie, "ETH3 IPC ")
    par_acteur = {role: list(map(int, re.search(role + r"=\[([^]]+)\]", acteurs)[1].split(",")))
                  for role in ("client", "service", "pilote")}
    appels = sum(sum(cols) for cols in par_acteur.values())
    # Les colonnes cinq et six recensent FEATURES et les autres entrées ABI.
    ipc = sum(sum(cols[:4]) for cols in par_acteur.values())
    return dict(lots=lots, colonnes_par_acteur=par_acteur, appels_comptes=appels,
                appels_ipc=ipc, ipc_par_trame_emise=ipc / preuve["trames_par_sens"],
                appels_par_trame_emise=appels / preuve["trames_par_sens"],
                debit_pair_Mbit_s=preuve["octets_par_sens"] * 8000 / preuve["intervalle_pair_ns"],
                changements=bilan["changements"], reveils=bilan["reveils"],
                appels_pilote=bilan["appels_pilote"], ticks_cpu=bilan["cpu_ticks"],
                ticks_idle=bilan["idle_ticks"], mediation_ticks=entiers(acteurs)["mediation_ticks"],
                cpu_hote=preuve["cpu_qemu"])


def main(archive):
    result = {"profils": {}, "couts_ms": {}, "ethernet": {}, "demarrages": 0}
    mesures = {nom: [] for nom in PHASES}
    stockage = {"lectures": [], "flush": []}
    plateformes = set()
    retours_mmio = set()
    sources = {}
    piles = {}
    memoire = {}
    for cas in sorted(CASES):
        rows = []
        perf = []
        for i in range(1, 4):
            dossier = archive / cas / str(i)
            c = json.loads((dossier / "campagne.json").read_text())
            s = (dossier / "serie.log").read_text()
            exiger(c["succes"] and c["elf_relu_esp"] == c["elf"] and c["volume_relu_identique"]
                   and not c["erreur_pair"] and not c["sources_suivies_modifiees"], f"{cas}/{i}")
            sources.setdefault(cas, set()).add(c["commit_sources"])
            profil = tuple(c[k] for k in ("modele", "firmware", "limine_arm", "distribution", "relais"))
            if cas.startswith("mmio-"):
                exiger(c["modele_recu"]["mode"] == cas.removeprefix("mmio-"), "modele MMIO inattendu")
                retours_mmio.add(profil)
            else:
                plateformes.add(profil)
            cycles = [list(map(int, x)) for x in re.findall(BILAN, s)]
            ram = [entiers(v) for v in s.splitlines() if v.startswith("RAM phase=4d_fin_cycle ")]
            if cas not in (TEMOIN, STATIQUE):
                exiger(len(ram) == len(cycles) > 0 and all(x["ipc"] == x["utilisateur"] == x["processus"] == 0
                       for x in ram), f"ressources résiduelles {cas}/{i}")
            if cas in NEG:
                exiger(c["refus_4d"]["cause"] == cas and not c["trames_par_sens"], f"refus {cas}")
            if cas == ENDURANCE:
                hz = entiers(ligne(s, "D4 phases "))["horloge"]
                exiger(len(cycles) == 256 and all(a == z and ipc == 0 and dma == 16
                       for a, _, z, _, ipc, dma in cycles), "endurance")
                for nom, regexp in PHASES.items():
                    xs = [int(n) * 1000 / hz for n in re.findall(regexp, s)]
                    exiger(len(xs) == len(cycles), f"phase {nom}/{i}")
                    mesures[nom].extend(xs)
                for nom, taille in (("lectures", 64), ("flush", 16)):
                    series = re.findall(r"NET ARM " + nom + r" [^\n]*?cycles=\[([^]]+)\]", s)
                    exiger(len(series) == len(cycles), f"stockage {nom}/{i}")
                    for samples in series:
                        values = [int(x) * 1000 / hz for x in samples.split(",")]
                        exiger(len(values) == taille, "série de stockage tronquée")
                        stockage[nom].extend(values)
            if cas in PERF:
                perf.append(profil_perf(s, c))
            if cas not in NEG and cas not in (TEMOIN, STATIQUE):
                for role, touche in re.findall(r"D4 pile role=(\w+) .*?pile_bloc_touche=(\d+)", s):
                    piles[role] = max(piles.get(role, 0), int(touche))
                for role, texte in (("gestionnaire", "D4 manager "), ("lanceur", "D4 pair ")):
                    piles[role] = max(piles.get(role, 0), entiers(ligne(s, texte))["pile_bloc_touche"])
            rfin = entiers(ligne(s, "RAM phase=net_final ")) if "RAM phase=net_final " in s else None
            rows.append(dict(demarrage=i, trames_par_sens=c["trames_par_sens"], octets_par_sens=c["octets_par_sens"],
                             elf_sha256=c["elf"], transport=c.get("transport_client"), cycles=len(cycles),
                             pic_cadres=max((x[3] for x in cycles), default=None),
                             delta_cadres_consommes_max=max((a-z for a, _, z, *_ in cycles), default=None),
                             retenus=c.get("cycle_vie", {}).get("memoire_finale"), ram_fin=rfin,
                             refus=c.get("refus_4d")))
            if i == 1 and cas in ("reset-court", "plein", "six-boites", TEMOIN, STATIQUE):
                phases = {k: entiers(v) for k, v in re.findall(r"RAM phase=(\w+) ([^\n]+)", s)}
                memoire[cas] = dict(phases=phases)
                if cas not in (TEMOIN, STATIQUE):
                    memoire[cas].update(gestionnaire=entiers(ligne(s,"D4 manager ")),
                                        lanceur=entiers(ligne(s,"D4 pair ")), tas=entiers(ligne(s,"D4 tas ")),
                                        selection=entiers(ligne(s,"D4 physique phase=selection ")),
                                        coexistence=entiers(ligne(s,"D4 coexistence ")))
        result["profils"][cas] = rows
        if perf:
            result["ethernet"][cas] = perf
        result["demarrages"] += len(rows)
    exiger(len(plateformes) == 1, "firmwares ou modèles différents")
    exiger(len(retours_mmio) == 1 and next(iter(retours_mmio))[1:] == next(iter(plateformes))[1:], "banc MMIO différent")
    result["modele_retours_mmio"] = retours_mmio.pop()[0]
    for cas, refs in sources.items():
        exiger(len(refs) == 1, f"révisions divergentes {cas}")
    exiger(len({next(iter(refs)) for cas, refs in sources.items() if cas != TEMOIN}) == 1,
           "sources dynamiques différentes")
    result["reference"] = next(iter(sources[ENDURANCE]))
    result["temoin"] = next(iter(sources[TEMOIN]))
    result["plateforme"] = dict(zip(("modele","firmware","limine_arm","distribution","relais"), plateformes.pop()))
    result["couts_ms"] = {k: quantiles(v) for k,v in mesures.items()}
    result["stockage_endurance_ms"] = {k: quantiles(v) for k,v in stockage.items()}
    result["memoire"] = memoire
    result["piles_touchees_observees_octets"] = piles
    med = statistics.median
    result["ethernet_medianes"] = {
        cas: {b: {k: med(x["lots"][b][k] for x in series)
                  for k in ("debit_Mbit_s", "p50_ms", "p95_ms", "p99_ms")}
              for b in ("1", "4", "8")}
        for cas,series in result["ethernet"].items()
    }
    return result


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=Path)
    p.add_argument("sortie", type=Path)
    args = p.parse_args()
    args.sortie.write_text(json.dumps(main(args.archive), ensure_ascii=False, indent=2) + "\n")
