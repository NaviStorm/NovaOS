"""Construire les tableaux à partir des sorties effectivement recueillies."""
import argparse
import json
from pathlib import Path
import shlex
import statistics

HERE=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument("campagne",type=Path)
args=parser.parse_args()
read=lambda p:json.loads(p.read_bytes())
rows=read(HERE/"preuves/options.json")
darwin=read(HERE/"preuves/darwin.json")
assert len(rows)==90 and all(r["ok"] for r in rows)
guests={}
serials=[]
summary=["# Mesures et qualification du 17 novembre 2026","",
         "Base Git `425e4e7cc5c03e246efb64d3638376906344bf47`. Portage `c63151a7116f7f0bc3130520e3c8be465ef4d763`. "
         "Le SHA-256 des trois binaires est consigné dans [l’environnement](preuves/environnement.txt).","",
         "## macOS ARM64","","M4 Max, Darwin natif, GNU 9.12 et Nova 0.24.0. "
         "Chaque ligne mesure 256 créations de même type par passage, sur trois passages par exécutable en alternant l’ordre. "
         "Le temps est la médiane murale du lot. La mémoire est le maximum des RSS individuelles observées avec wait4, en Kio.","",
         "| Objet | Processus | Temps Nova (ms) | Temps GNU (ms) | Nova (créations/s) | GNU (créations/s) | RSS Nova | RSS GNU |",
         "|---|---:|---:|---:|---:|---:|---:|---:|"]
for is_dir in (False,True):
    for w in (1,4):
        values=[]
        for gnu in (False,True):
            group=[r for r in darwin["mesures"] if r["dossier"]==is_dir and r["paralleles"]==w and (Path(r["exe"]).name=="gmktemp")==gnu]
            assert len(group)==3 and all(r["n"]==len(set(r["noms"]))==256 for r in group)
            ns=statistics.median(r["ns"] for r in group)
            values.append([f"{ns/1e6:.1f}",f"{256e9/ns:.0f}",str(max(r["rss_max"] for r in group)//1024)])
        a,b=values
        summary.append("| "+" | ".join(["Dossier" if is_dir else "Fichier",str(w),a[0],b[0],a[1],b[1],a[2],b[2]])+" |")
summary+=["","90/90 cas différentiels. Le test GNU mktemp.pl compte 33 cas par exécutable. "
          "Un test distinct conserve le lien symbolique occupé et son fichier témoin pour les trois modes. "
          "Les essais Linux strace/setarch et /dev/full ne sont pas disponibles sur ce Mac. "
          "Le banc ferme stdout pour vérifier la restitution du nom et le nettoyage. APFS refuse le suffixe d’octet FF sur les deux programmes.","",
          "## NovaOS sous QEMU","","TCG, deux vCPU, 256 Mio de RAM, disque VirtIO P9 jetable. "
          "Chaque lot comporte quatre fichiers et quatre répertoires par demandeur. "
          "Les débits sont calculés à partir de trois durées murales, délimitées par la série. "
          "Les mesures de l’ELF comprennent l’échange de requête et réponse par P9, le lancement et la collecte des huit travailleurs. "
          "Les noms sont relus après la collecte. En concurrence, les intervalles des deux demandeurs se recouvrent à chaque passage. "
          "La sonde de collisions impose un premier nom occupé par création dans un seul travailleur par lot. "
          "Le pic de cadres de 4 Kio couvre toute la session de stockage dans la VM. La RSS isolée de mktemp y reste non mesurée.","",
          "| Invité | Secteur | ELF série (créations/s) | ELF, 2 demandeurs (créations/s) | Sonde avec collisions (créations/s) | Pic de cadres, série / duo |",
          "|---|---:|---:|---:|---:|---:|"]
for arch in ["aarch64","x86_64"]:
    for sector in [512,4096]:
        suites={}
        for name in ["nsh","elf","prive","commande","duo"]:
            folder=args.campagne/f"{name}-{arch}-{sector}"
            result=read(folder/"resultats.json")
            assert (result["arch"],result["secteur"])==(arch,sector)
            suites[name]=result
            guests[folder.name]=result
        single,dual,engine=[suites[k] for k in ("commande","duo","prive")]
        serials.append(single["matrice"])
        nsh=suites["nsh"]
        assert nsh["avant"]==nsh["apres"] and nsh["commandes"][-1]["status"]==3
        assert dual["objets_par_passage"]==16 and engine["collisions_forcees_par_passage"]==8
        summary.append(f'| {arch} | {sector} | {single["creations_s"]:.1f} | {dual["creations_s"]:.1f} | {engine["creations_s"]:.1f} | {single["pic_cadres_session"]} / {dual["pic_cadres_session"]} |')
assert serials.count(serials[0])==4
matrix=serials[0]
summary+=["",f'{len(matrix["indices"])}/90 cas de la matrice Darwin sont rejoués par le véritable ELF sur chacun des quatre profils. '
          "Les absences de correspondance figurent dans la matrice. Les quatre profils vérifient aussi un chemin fragmenté et le refus de remontée. "
          "Le shell ordinaire donne 0 pour aide, version et manuel en français et en anglais selon LANG, 127 pour la commande métier et 3 pour le pipe mktemp -u | wc -l. "
          "Aucune variation de l’empreinte P9 ni des compteurs WRITE/FLUSH pendant les consultations. "
          "Le profil SDK sans ports refuse la sortie des sept invocations. GNU n’a pas d’exécutable pour l’ABI native.","",
          "Les chronométrages d’ELF de recette sont propres à la machine virtuelle. "
          "Le lanceur générique et les droits de flux du shell restent à intégrer. "
          "Le profil concurrent porte sur deux instances, chacune propriétaire de sa vue privée. "
          "Une panne par coupure secteur pendant la préparation et la comparaison des traductions gettext hors locale C restent hors qualification.","",
          "[Mesures Darwin brutes](preuves/darwin.json) · [Résultats et chronométrages des vingt démarrages](preuves/qemu.json) · [Matrice détaillée](MATRICE.md)"]
(HERE/"RESULTATS.md").write_text("\n".join(summary)+"\n")
(HERE/"preuves/qemu.json").write_text(json.dumps(guests,ensure_ascii=False,indent=1)+"\n")
lines=["# Matrice des options, alias et combinaisons","",
       "Chaque ligne donne le statut de GNU et de Nova sur Darwin, puis l’observation dans le travailleur NovaOS sur les quatre géométries/ISA. "
       "La colonne effet contient le motif de stdout avant LF ou le diagnostic GNU en locale C. "
       "Pour un succès métier, le contrôle inclut l’existence, le type et le mode sur Darwin, ou le type et la réouverture dans la vue P9. "
       "Pour un échec métier de stockage, NovaOS contrôle le statut et la présence du diagnostic natif. "
       "La colonne scénario mentionne toute variation d’environnement ou fermeture de stdout. "
       "Les octets effectivement lus sur Darwin figurent dans [les sorties brutes](preuves/options.json).","",
       "| Cas | Arguments | Scénario | GNU / Nova Darwin | Effet ou diagnostic | ELF NovaOS |",
       "|---:|---|---|---:|---|---|"]
for i,r in enumerate(rows):
    argv=shlex.join([bytes.fromhex(w).decode("utf8","backslashreplace") for w in r["argv"]])
    scenario="stdout fermé" if r["closed"] else ", ".join(k+"="+v for k,v in r["env"].items()) or "C, TMPDIR=env"
    err=bytes.fromhex(r["gnu"]["err"]).decode("utf8","backslashreplace").splitlines()
    effect=r["gnu"]["effect"]
    label={"dir":"dossier 0700","file":"fichier 0600"}[effect[0]] if effect else "sans création"
    outcome=r["doc"] or (label+" · "+r["pattern"] if r["pattern"] else (err[0] if err else "stderr vide"))
    guest="4/4" if i in matrix["indices"] else matrix["exclus"][str(i)]
    lines.append(f'| {i+1} | `{argv or "(aucun)"}` | {scenario} | {r["gnu"]["rc"]} / {r["nova"]["rc"]} | `{outcome}` | {guest} |')
(HERE/"MATRICE.md").write_text("\n".join(lines)+"\n")
