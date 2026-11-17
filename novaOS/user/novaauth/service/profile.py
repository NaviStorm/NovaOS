"""Prépare une copie de compilation du commit, avec le raccord NovaAuth réservé à QEMU."""
import argparse
from pathlib import Path
import hashlib
import re
import runpy
import subprocess
import tarfile

def prepare(destination, policies=False, retired_reply=False, shell=False, selection_smp=False, bail=False, budget=False, transport=False, incarnation=False, horloge=False):
    incarnation = incarnation or horloge
    if budget or transport or incarnation:
        shell = bail = True
    source = Path(__file__).resolve().parents[4]
    revision = subprocess.check_output(["git","rev-parse","HEAD"],cwd=source,text=True).strip()
    destination.mkdir(parents=True,exist_ok=False)
    # Les sources reproductibles viennent du commit désigné. Aucun index de
    # worktree et aucun fichier personnel ou ignoré n'entre dans cette copie.
    paths=["novaOS","novaFS","components"]
    if shell:
        # La suite ABI parcourt également la sonde CUT suivie dans le dépôt.
        paths.append("livraison-cut-0240")
    with subprocess.Popen(["git","archive",revision,*paths],cwd=source,stdout=subprocess.PIPE) as archive:
        with tarfile.open(fileobj=archive.stdout,mode="r|") as stream:
            stream.extractall(destination,filter="data")
        if archive.wait()!=0: raise RuntimeError("export Git incomplet")
    apply(destination, "raccord.patch")
    if shell:
        apply(destination, "lancement.patch")
        runpy.run_path(str(destination / "novaOS/user/novaauth/service/limine_hhdm_qemu.py"))["install"](destination)
        apply(destination, "session-expiration.patch")
        if bail:
            apply(destination, "bail.patch")
        if selection_smp:
            apply(destination, "selection-smp.patch")
        if budget or transport or incarnation:
            apply(destination, "budget-autorite.patch")
        if incarnation:
            apply(destination, "incarnation-invite.patch")
        if horloge:
            apply(destination, "transport-incarnation.patch")
            apply(destination, "horloge-signataire.patch")
        if transport:
            apply(destination, "sonde-transport.patch")
        refresh_documentation_links(destination)
    if policies:
        apply(destination, "politiques.patch")
    if retired_reply:
        apply(destination, "reponse-retiree.patch")
    print(revision,destination)

def refresh_documentation_links(destination):
    """Les lignes des gestionnaires changent dans la copie instrumentée."""
    generator = runpy.run_path(str(destination / "novaOS/scripts/generate_abi.py"))
    for path, content in generator["outputs"]().items():
        previous = path.read_text()
        if content == previous:
            continue
        line_anchor = r"#L[0-9]+(?=\))"
        if path != destination / "novaOS/docs/registre-abi.md" or (
            re.sub(line_anchor, "#L", content) != re.sub(line_anchor, "#L", previous)
        ):
            raise ValueError(f"Le profil change le contrat ABI : {path}")
        path.write_text(content)


def apply(destination,name):
    patch=destination/"novaOS/user/novaauth/service"/name
    for path, preimage in re.findall(r"^diff --git a/(\S+) b/\S+\nindex ([0-9a-f]{40})\.\.[0-9a-f]{40}",patch.read_text(),re.M):
        content=(destination/path).read_bytes()
        oid=hashlib.sha1(b"blob "+str(len(content)).encode()+b"\0"+content).hexdigest()
        if oid!=preimage: raise ValueError(f"source différente de la référence du raccord: {path}")
    subprocess.run(["git","apply","--unidiff-zero","--check",str(patch)],cwd=destination,check=True)
    subprocess.run(["git","apply","--unidiff-zero",str(patch)],cwd=destination,check=True)
if __name__=="__main__":
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination",type=Path)
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--politiques", action="store_true")
    mode.add_argument("--reponse-retiree", action="store_true")
    mode.add_argument("--lancement", action="store_true")
    mode.add_argument("--selection-smp", action="store_true", help="profil de lancement avec sonde SMP invitée du raccord privé")
    mode.add_argument("--bail", action="store_true", help="profil de lancement avec horodatage de la décision NovaAuth et témoin de réauthentification")
    mode.add_argument("--horloge-signataire", action="store_true", help="lecture monotone bornée et canal TLS du signataire de qualification")
    mode.add_argument("--incarnation-registre", action="store_true", help="preuve de possession du signataire et du registre isolés sur la route privée")
    mode.add_argument("--budget-autorite", action="store_true", help="deux processus de mesure des routes privées")
    mode.add_argument("--transport-autorite", action="store_true", help="sonde de transport hors P9 sur export à 514 pages")
    args=parser.parse_args()
    prepare(args.destination.resolve(), args.politiques, args.reponse_retiree, args.lancement or args.selection_smp or args.bail,
            args.selection_smp, args.bail,
            args.budget_autorite, args.transport_autorite, args.incarnation_registre, args.horloge_signataire)
