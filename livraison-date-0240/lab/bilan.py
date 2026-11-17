"""Rend les tableaux exclusivement à partir des observations enregistrées."""
import gzip
import hashlib
import json
from pathlib import Path
import statistics

HERE = Path(__file__).resolve().parents[1]
proof = HERE / "preuves"
report = json.loads((proof/"mesures.json").read_text())
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
assert report["Nova_SHA256"] == digest(HERE/"bin/date")
lines = [
    "# Mesures de date",
    "",
    f"{report['cpu']}, {report['systeme']}, {report['machine']}. Locale C, TZ=UTC0. "
    f"{report['repetitions']} mesures chronométrées après un échauffement, en alternant les deux processus.",
    "",
    "| Lot macOS | Dates | Nova ms | GNU ms | Nova dates/s | GNU dates/s | Nova RSS Mio | GNU RSS Mio |",
    "|---|---:|---:|---:|---:|---:|---:|---:|"]
for case in report["charges"]:
    a,b=case["Nova"],case["GNU"]
    lines.append(f"| {case['charge']} | {case['dates']} | {a['mediane']*1000:.2f} | {b['mediane']*1000:.2f} | {a['debit']:.0f} | {b['debit']:.0f} | {a['rss']/2**20:.2f} | {b['rss']/2**20:.2f} |")
lines.extend(["","La durée couvre le démarrage du processus, la lecture, l’analyse, le formatage et la sortie redirigée vers le périphérique nul. Le RSS est le pic du processus enfant fourni par wait4, puis la médiane des passages. Les empreintes des sorties utiles ou des diagnostics et les statuts des deux outils ont été comparés avant chronométrage.","",
              "## NovaOS sous QEMU","","QEMU 11.1.3, TCG, un vCPU, 256 Mio, VirtIO-blk et volume P9 jetable. AArch64 virt Cortex-A72/GICv2 et x86_64 Q35/max. Un passage de chauffe puis trois observations. Chaque lot contient 32 lignes. Le lancement utilise la délégation de la recette.",
              "","| ISA | Secteur | Lot | Dates | Nova ms, session | Nova dates/s, session | Nova ms, processus | Pages utilisateur au pic | Kio |",
              "|---|---:|---|---:|---:|---:|---:|---:|---:|"])
for arch in ("aarch64","x86_64"):
    for sector in (512,4096):
        r=json.load(gzip.open(proof/f"lot-{arch}-{sector}.json.gz"))
        assert r["elf"]==digest(HERE/"bin"/f"date-{arch}.elf")
        for case in r["cas"]:
            if "charge" not in case:continue
            measures=case["processus"][1:]
            pages=statistics.median(x["pages"] for x in measures)
            elapsed=statistics.median(x["secondes"] for x in measures)
            lines.append(f"| {arch} | {sector} | {case['charge']} | {case['dates']} | {case['mediane']*1000:.2f} | {case['debit']:.0f} | {elapsed*1000:.2f} | {pages:g} | {pages*4:g} |")
lines.extend(["","Le temps de session inclut nsh, UART et le retour du statut. Le temps de processus va du premier appel observé à la sortie, à partir du compteur invité. Le pic mémoire échantillonne les pages privées de code et de données aux appels système. Il exclut les tables de pages, les services, QEMU et le noyau. Les pages sont de 4 Kio. L’oracle GNU tourne sur Darwin. La campagne ne contient aucune mesure de vitesse de GNU dans NovaOS.","","[Observations Darwin](preuves/mesures.json) · [Observations QEMU](preuves/)"])
(HERE/"PERFORMANCES.md").write_text("\n".join(lines)+"\n")
