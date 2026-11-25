"""Conformité RX des fonctions QEMU amont, compilées sans modification pour l'hôte."""
import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
import tempfile
from pathlib import Path

TESTS = Path(__file__).resolve().parent


def extraire(texte, prefix, suffix):
    match = re.search(prefix + r".*?" + suffix, texte, re.MULTILINE | re.DOTALL)
    if not match:
        raise ValueError(f"Définition amont absente : {prefix}")
    return match[0] + "\n"


def lire(version, sources, chemin, manifeste):
    fichier = sources / version / Path(chemin).name
    contenu = fichier.read_bytes()
    url = f"https://raw.githubusercontent.com/qemu/qemu/{version}/{chemin}"
    if hashlib.sha256(contenu).hexdigest() != manifeste[url]:
        raise ValueError(f"Empreinte amont incorrecte : {fichier}")
    return contenu.decode()


def tester(version, sources, manifeste, copie=None):
    net = lire(version, sources, "hw/net/virtio-net.c", manifeste)
    entetes = lire(version, sources, "include/standard-headers/linux/virtio_net.h", manifeste)
    with tempfile.TemporaryDirectory() as work:
        tmp = Path(work)
        if copie is not None:
            ref = tmp / "virtio-net.c"
            ref.write_text(net)
            diff = TESTS.parent / "docs/validation/virtio-net/ethernet-20261125/proposition-num-buffers.diff"
            if hashlib.sha256(diff.read_bytes()).hexdigest() != "64bba00421bea605521de0514a5639730fa61fee2e1de2ee55480608bec832ef":
                raise ValueError("Diff de revue différent")
            with diff.open("rb") as entree:
                subprocess.run(["patch", "-s", "-F", "0", str(ref)], stdin=entree, check=True)
            if ref.read_bytes() != copie.read_bytes():
                raise ValueError("La copie compilée diffère du seul correctif approuvé")
            net = copie.read_text()
        structs = ["virtio_net_hdr_v1", "virtio_net_hdr_v1_hash", "virtio_net_hdr"]
        (tmp / "entetes-amont.inc").write_text("".join(
            extraire(entetes, rf"^struct {nom} \{{", r"^\}[^;\n]*;") for nom in structs))
        (tmp / "reception-amont.inc").write_text(
            extraire(net, r"^static void receive_header\(", r"^\}") +
            extraire(net, r"^static ssize_t virtio_net_receive_rcu\(", r"^\}"))
        exe = tmp / "reception"
        subprocess.run([os.environ.get("CC", "cc"), "-std=gnu11", "-O2",
                        "-I", str(tmp), str(TESTS / "virtio_net_rx_modele.c"), "-o", str(exe)],
                       check=True)
        lines = subprocess.check_output([exe], text=True).splitlines()
    counts = {}
    for line in lines:
        m = re.fullmatch(r"cas=([^ ]+) num_buffers=(\d+) longueur=72 contenu=conforme", line)
        if m:
            counts[m[1]] = int(m[2])
    maxima = {m[1]: int(m[2]) for line in lines
              if (m := re.fullmatch(r"cas=(fusion-1514|simple-1514) num_buffers=(\d+) longueur=1526 contenu=conforme", line))}
    if set(maxima) != {"fusion-1514", "simple-1514"} or maxima["fusion-1514"] != 1:
        raise ValueError(f"Témoin de taille maximale incomplet : {lines}")
    if (set(counts) != {"moderne-simple", "moderne-fusion", "témoin-mémoire", "sg-10", "sg-11", "legacy-fusion", "fusion-deux"}
            or counts["moderne-fusion"] != 1 or counts["legacy-fusion"] != 1 or counts["fusion-deux"] != 2
            or "cas=sans-tampon aucune-completion" not in lines
            or "cas=partiel abandon aucune-completion" not in lines or "cas=legacy-simple longueur=70 contenu=conforme" not in lines):
        raise ValueError(f"Témoin positif ou résultat RX incomplet : {lines}")
    return {"version": version, "traces": lines, "num_buffers": counts["moderne-simple"],
            "attendu_sans_fusion": 1, "conforme_sans_fusion": counts["moderne-simple"] == counts["témoin-mémoire"] == counts["sg-10"] == counts["sg-11"] == maxima["simple-1514"] == 1}


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("sources", type=Path)
    p.add_argument("manifeste", type=Path)
    p.add_argument("--copies", nargs=2, type=Path, metavar=("QEMU10", "QEMU11"))
    args = p.parse_args()
    manifeste = json.loads(args.manifeste.read_text())
    essais = [tester(v, args.sources, manifeste, copie)
              for v, copie in zip(("v10.0.12", "v11.1.3"), args.copies or [None, None])]
    print(json.dumps({"hote": platform.machine(), "correctif": bool(args.copies), "portee": "fonctions C, file simulée, aucun DMA",
                      "essais": essais}, ensure_ascii=False, indent=2))
    raise SystemExit(0 if all(r["conforme_sans_fusion"] for r in essais) else 2)
