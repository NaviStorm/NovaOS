"""Tests hôtes ciblés du contrat Ethernet, des retraits concurrents et du registre ABI."""
import argparse
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import tomllib

from port_construire_q35 import AUDIT, NOVA, git
from port_qemu import qualification

FILTRES = ("credits_and_journal_saturation_recovery", "publication_incertaine_conserve_credit_et_recu",
           "revoke_during_lot_scrubs_and_preserves_unknown", "lost_response_and_partial_publication",
           "atomic_batch_and_generations", "identities_ports_and_incarnations",
           "armed_wait_wakes_once_including_revoke", "versions_features_and_initial_denial",
           "snapshot_padding_and_remote_delivery_oracle", "retrait_dma::")


def lancer(commande, sortie):
    with sortie.open("xb") as log:
        subprocess.run(commande, stdout=log, stderr=subprocess.STDOUT, check=True)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie", type=lambda v:Path(v).resolve())
    dest = p.parse_args().sortie
    dest.mkdir(parents=True, exist_ok=False)
    tmp = Path(os.environ["NOVAOS_TMPDIR"]).resolve(strict=True)
    bilan = dict(revision=git("rev-parse","HEAD").decode().strip(), audit=AUDIT, essais=[])
    with qualification(tmp.parent, bilan), tempfile.TemporaryDirectory(dir=tmp) as td:
        src = Path(td)
        fichiers = ("tests/virtio_net_retraits.rs", "kernel/src/hardware/nvme_domain/traffic/requests.rs",
                    "kernel/src/hardware/vtd_lab/format.rs")
        for nom in fichiers:
            cible = src/nom
            cible.parent.mkdir(parents=True, exist_ok=True)
            cible.write_bytes(git("show", f"{AUDIT}:novaOS/{nom}"))
        (src/"vtd.rs").write_text('#[path = "kernel/src/hardware/vtd_lab/format.rs"]\nmod format;\n')
        for nom,entree,filtres in (("ethernet",NOVA/"tests/ethernet_v1/lib.rs",FILTRES),
                                    ("demandes",src/fichiers[0],()), ("vtd",src/"vtd.rs",())):
            for arch,cible in (("arm64","aarch64-apple-darwin"), ("x86_64","x86_64-apple-darwin")):
                binaire = dest/f"{nom}-{arch}"
                cmd = ["rustup","run","stable","rustc","--edition","2024","--test","-O",
                       "--target",cible,str(entree),"-o",str(binaire)]
                lancer(cmd, dest/f"{nom}-{arch}-compilation.log")
                exe = ["arch","-"+arch,str(binaire),"--test-threads=1",*filtres]
                log = dest/f"{nom}-{arch}.log"
                lancer(exe,log)
                n = int(re.search(r"test result: ok\. (\d+) passed",log.read_text())[1])
                statut = subprocess.check_output(["arch","-"+arch,"sysctl","-in","sysctl.proc_translated"],text=True).strip()
                if n == 0 or (arch=="x86_64") != (statut=="1"):
                    raise ValueError("Architecture hôte ou sélection des tests")
                bilan["essais"].append(dict(jeu=nom,execution=exe,compilation=cmd,architecture=arch,
                    rosetta=statut=="1",tests=n,elf_sha256=sha256(binaire.read_bytes()).hexdigest()))
        for script,opts in (("tests/abi_registre.py",[]), ("scripts/generate_abi.py",["--check"])):
            lancer([sys.executable,"-B",str(NOVA/script),*opts],dest/(Path(script).stem+".log"))
        registre=tomllib.loads((NOVA/"abi/syscalls.toml").read_text())
        bilan["selecteurs"]=len(registre["call"])
        if bilan["selecteurs"] != 48:
            raise ValueError("Nombre de sélecteurs")
    (dest/"bilan.json").write_text(json.dumps(bilan,indent=2)+"\n")


if __name__ == "__main__":
    main()
