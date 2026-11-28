"""Contre-lecture hors VM des observations 5L, sans exécuter les pièces archivées."""

import argparse
import importlib.util
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import struct
import sys
import tarfile
from tempfile import TemporaryDirectory

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5k"))
from verdict_apic import juger

EFI = ("nominal", "inactif", "nominal-refus", "inactif-refus")
DISQUES = ("actif-wait", "actif-iqe", "duo-wait-absent", "duo-wait-tardif")
GUID = "33d532ede69909429cc02d72cdd998a7"
ABSENTE = (1 << 63) + 14


def check(ok, msg):
    if not ok:
        raise ValueError(msg)


def pages_modifiees(a, b, spans):
    check(
        all(
            0 <= lo < hi <= 256 << 20 and not lo % 4096 and not hi % 4096
            for lo, hi in spans
        )
        and all(g[1] <= d[0] for g, d in zip(spans, spans[1:])),
        "Étendues RAM",
    )
    pages = [p for lo, hi in spans for p in range(lo, hi, 4096)]
    check(len(a) == len(b) == 32 * len(pages) and pages, "Couverture des clichés")
    return [
        p
        for i, p in enumerate(pages)
        if a[32 * i : 32 * i + 32] != b[32 * i : 32 * i + 32]
    ]


def smi(buf, out, retour, control):
    attendu = control + 0x1465
    apm = buf[out:retour]
    check(
        re.search(
            rb"(?m)^memory_region_ops_write cpu 0 mr 0x[0-9a-f]+ addr 0xb2 value 0x0 size 1 name 'apm-io'$",
            apm,
        )
        and apm.count(b"apm_io_write write addr=0x0 val=0x00\n") == 1,
        "Commande APM",
    )
    labels = re.findall(
        rb"(?m)^SMM: (enter|after RSM)\n[\s\S]*?^RIP=([0-9a-f]+) [^\n]+ SMM=0 ", apm
    )
    check(
        labels
        == [
            (b"enter", f"{attendu:016x}".encode()),
            (b"after RSM", f"{attendu:016x}".encode()),
        ]
        and apm.count(b"SMM: ") == 2,
        "Entrée et retour SMM",
    )


def transition(nom, lire):
    d = json.loads(lire("observation.json"))
    proof = json.loads(lire("apic.json"))
    t = lire("trace.log")
    inter = lire("intervalle.log")
    result = juger(t, inter, proof)
    check(
        result == d["verdict"]
        and not result["intervalle_admis"]
        and len(result["ecritures_refusees"]) == 6,
        "Verdict EFI",
    )
    check(
        inter.count(b"SMM: enter\n") == inter.count(b"SMM: after RSM\n") == 3
        and inter.count(b"apm_io_write write") == 6,
        "SMI surnuméraire",
    )
    for s in (
        b"pci_cfg_write ",
        b"vtd_reg_write ",
        b"pci_nvme_io_cmd ",
        b"vtd_dmar_fault ",
        b"virtio_notify ",
    ):
        check(s not in inter, "Modification ou accès périphérique")
    events = d["evenements"]
    check(
        len(events) == 15
        and 0 <= events[0]["offset"] <= events[-1]["offset"] <= len(inter)
        and all(a["offset"] <= b["offset"] for a, b in zip(events, events[1:])),
        "Chronologie",
    )
    check(
        d["image"] % 4096 == 0
        and d["image_fin"] - d["image"] == 0x44D000
        and any(lo <= d["image"] < d["image_fin"] <= hi for lo, hi in d["ram"]),
        "Couverture des pages du diagnostic",
    )
    changes = []
    for n, flag in enumerate(("SecureBoot", "SetupMode", "AuditMode")):
        get, data, cmd, ret, rep = events[n * 5 : n * 5 + 5]
        check(
            [x["role"] for x in (get, data, cmd, ret, rep)]
            == ["variable", "data", "commande", "retour", "reponse"]
            and get["nom"] == flag
            and get["pc"] == d["get"]
            and rep["pc"] == get["retour"],
            "Attribution GetVariable",
        )
        check(
            (data["pc"], cmd["pc"], ret["pc"])
            == tuple(d["controle"] + o for o in (0x1454, 0x1464, 0x1465))
            and (data["port"], cmd["port"], data["valeur"], cmd["valeur"])
            == (0xB3, 0xB2, 0, 0),
            "Émetteur et largeurs",
        )
        seq = inter[data["offset"] : cmd["offset"]]
        check(
            re.search(
                rb"(?m)^memory_region_ops_write cpu 0 mr 0x[0-9a-f]+ addr 0xb3 value 0x0 size 1 name 'apm-io'$",
                seq,
            )
            and seq.count(b"apm_io_write write addr=0x1 val=0x00\n") == 1
            and b"SMM: enter" not in seq,
            "Donnée APM",
        )
        smi(inter, cmd["offset"], ret["offset"], d["controle"])
        check(
            all(
                x["guid"] == GUID
                and x["fonction"] == 1
                and x["taille"] == 53 + 2 * len(flag) + 2
                for x in (cmd, ret)
            )
            and ret["canal"] == cmd["canal"]
            and rep["statut"] == ret["statut_mm"] == (ABSENTE if n == 2 else 0)
            and rep["valeur"]
            == (None if n == 2 else int(n == 0 and nom.startswith("nominal"))),
            "Réponse du canal MM",
        )
        a, b = [lire(f"{n + 1}-{s}.sha-pages") for s in ("commande", "retour")]
        check(
            [sha256(x).hexdigest() for x in (a, b)]
            == [x["pages_sha256"] for x in (cmd, ret)],
            "Empreintes par page",
        )
        dirty = pages_modifiees(a, b, d["ram"])
        check(
            not any(d["image"] <= p < d["image_fin"] for p in dirty),
            "Modification nette dans le PE",
        )
        changes.append(dirty)
    pre = json.loads(lire("pci-avant.json"))
    check(
        pre == json.loads(lire("pci-apres.json"))
        and lire("tables-avant.bin") == lire("tables-apres.bin")
        and len(lire("tables-avant.bin")) == 14 * 4096,
        "PCI et VT-d",
    )
    b0, b1 = [
        {s["device"]: s["stats"] for s in json.loads(line)}
        for line in lire("compteurs-qmp.jsonl").splitlines()
    ]
    for dev in ("pflash0", "pflash1", "interne"):
        for k in ("wr_operations", "wr_bytes", "flush_operations"):
            check(b0[dev][k] == b1[dev][k], "Écriture bloc pendant EFI")
    check(
        all(
            v == 0
            for k, v in b1["interne"].items()
            if k.endswith(("_operations", "_bytes"))
        ),
        "E/S internes",
    )
    r = json.loads(lire("resultat.json"))
    epoch = bytes.fromhex(r["epoque"])
    marque, _, _, status, sb, _, vivants, echo = struct.unpack(
        "<16s6Q16s", lire("recu.bin")
    )
    refus = nom.endswith("-refus")
    check(
        marque == b"NOVA5J-RAM-FINAL"
        and lire("epoch.bin") == b"NOVA5J-BOOT-EPOC" + epoch
        and r["statut"] == status == (14 if refus else 0)
        and sb == int(nom.startswith("nominal"))
        and echo == (bytes(16) if refus else epoch)
        and not vivants,
        "Arrêt EFI",
    )
    return dict(
        epoque=r["epoque"],
        eoi=result["eoi_attribues"],
        pages_modifiees=changes,
        pages_couvertes=sum((b - a) // 4096 for a, b in d["ram"]),
    )


def domaine(d, data, iqe):
    ctl = d["controleur"]
    disk = d["disque"]
    nic = d["nic"]
    regs = d["registres"]
    check(
        len(ctl) == 8
        and len(disk) == 6
        and len(nic) == 26
        and len(set(ctl + disk + nic)) == 40
        and len(data) == 8 * 4096,
        "Propriétés",
    )

    def page(i):
        return {
            j: x for j, x in enumerate(struct.unpack_from("<512Q", data, i * 4096)) if x
        }

    check(
        page(0) == {0: ctl[1] | 1} and page(1) == {96: ctl[2] | 1, 97: 0x102},
        "Racine et contexte NVMe",
    )
    for i, shift in enumerate((39, 30, 21)):
        check(
            page(i + 2) == {(0x02000000 >> shift) & 511: ctl[i + 3] | 3}, "Table NVMe"
        )
    feuilles = page(5)
    check(
        set(feuilles) == set(range(6))
        and all(p == disk[i] | [1, 2, 1, 2, 2, 1][i] for i, p in feuilles.items()),
        "Permissions NVMe",
    )
    check(
        regs
        == dict(
            root=ctl[0],
            qi=ctl[6],
            iqh=160 if iqe else 176,
            iqt=192,
            gsts=0xC4000000,
            fsts=16 if iqe else 0,
            wait=0,
        )
        and not page(7),
        "QI matérielle",
    )
    lots = []
    for seq, (sid, did) in enumerate(((0, 0), (48, 1), (56, 3), (56, 3)), 1):
        lots.extend(
            (
                sid << 32 | did << 16 | 0x31 if sid else 0x11,
                0,
                0 if iqe and seq == 4 else (did << 16 | 0xE2 if sid else 0xD2),
                0,
                seq << 32 | 0x25,
                ctl[7],
            )
        )
    check(
        data[6 * 4096 : 7 * 4096] == struct.pack("<24Q", *lots).ljust(4096, b"\0"),
        "Portée des commandes QI",
    )


def chronometrie(serie, succes):
    valeur = b"Ok(())" if succes else b"Err(Quarantine)"
    mesures = re.findall(rb"NOVA5L invalidation resultat=(\S+) ticks=(\d+)", serie)
    frequences = re.findall(rb"NET Q35 L2 [^\r\n]+ hz=(\d+) dma=40", serie)
    check(
        len(mesures) == len(frequences) == 1 and mesures[0][0] == valeur,
        "Chronométrie QI",
    )
    ticks, hz = int(mesures[0][1]), int(frequences[0])
    check(ticks > 0 and hz > 0, "Horloge invitée")
    return round(ticks * 1e6 / hz, 2)


def construction(nom, lire):
    provenance = json.loads(lire("sources.json"))
    campagne = json.loads(lire("campagne.json"))
    check(
        campagne["elf"] == provenance["elf"][campagne["cas"]]
        and campagne["commit_sources"] == provenance["port"]
        and campagne["audit"] == provenance["audit"],
        "Origine de la copie instrumentée",
    )
    check(bool(provenance.get("iqe_active_5l")) == (nom == "actif-iqe"), "Profil IQE")
    attendu = sha256(
        (Path(__file__).with_name("sonde-apres-panne.rs.inc")).read_bytes()
    ).hexdigest()
    check(
        provenance.get("sonde_5l_sha256")
        == (attendu if nom.startswith("duo-wait-") else None),
        "Origine de la sonde de soumission",
    )
    return campagne


def stockage(nom, lire, reader):
    d = construction(nom, lire)
    s = d["stockage"]
    disk = lire("namespace.raw")
    trace = lire("qemu.trace")
    serie = lire("serie.log")
    old = nom.startswith("actif-")
    iqe = nom == "actif-iqe"
    late = nom.endswith("-tardif")
    marque = b"invalid inv desc" if iqe else b"NOVA WAIT hold mode="
    check(d["succes"] and trace.count(marque) == 1, "Faute absente")
    before, after = trace.split(marque)
    n = before.count(b"pci_nvme_io_cmd ")
    check(
        n == (1 if old else 32) and b"pci_nvme_io_cmd " not in after,
        "Commande NVMe après la panne",
    )
    check(
        sha256(disk).hexdigest() == d["p9_final"]
        and len(disk) == 4 << 20
        and sha256(disk[:-4096]).hexdigest() == s["p9"]["sha256_p9"]
        and sha256(disk[-4096:]).hexdigest() == s["p9"]["sha256_garde"]
        and s["p9"]["etat"] == ("ancien" if old else "nouveau"),
        "Intégrité P9",
    )
    check(
        b"NVME ressources initial=0 final=40" in serie
        and b"NVME bilan ok=false" in serie
        and b"liberes=26" not in serie,
        "Quarantaine",
    )
    if old:
        domaine(d["domaine_5l"], lire("tables-qi.bin"), iqe)
        cmd_nvme = re.findall(
            rb"pci_cfg_write nvme 00:06\.0 @0x4 <- (0x[0-9a-f]+)", before
        )
        check(
            cmd_nvme
            and int(cmd_nvme[-1], 16) & 4
            and b"pci_cfg_write nvme 00:06.0 @0x4 <- 0x402" in after,
            "BME du stockage à la panne et à l’arrêt",
        )
        check(
            d["graine"] == d["p9_final"]
            and b"NVME P9 statut bloc=0b" in serie
            and b"panne NIC non isol" in serie
            and not d["pci_final"]["48"] & 4,
            "Arrêt et BME",
        )
    else:
        r = d["qi_incertaine"]
        check(
            r["cookie"] == (4 if late else 0)
            and r["iqh"] == (192 if late else 176)
            and r["iqt"] == 192
            and r["contexte"]["97"] == 0x102
            and trace.count(b"NOVA WAIT release cookie=4") == int(late),
            "WAIT et domaine",
        )
        check(
            serie.count(
                b"NOVA5L post_panne read=Err(State) flush=Err(State) cid_avant=None cid_apres=None cadres=40"
            )
            == 1
            and b"deux_retraits_refuses" in serie,
            "Demandes après la panne",
        )
    if reader:
        from nvme_phase2_oracle import inspect

        check(
            sha256(reader.read_bytes()).hexdigest() == d["lecteur"], "Identité lecteur"
        )
        with TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as td:
            f = Path(td) / "namespace.raw"
            f.write_bytes(disk)
            check(
                json.loads(json.dumps(inspect(reader, f, 512))) == s["p9"],
                "Remontage indépendant",
            )
    return dict(
        invalidation_ou_detection_us=chronometrie(serie, False),
        commandes_avant=n,
        commandes_apres=0,
        etat=s["p9"]["etat"],
        cadres_retenus=40,
    )


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "--dossier", type=Path, default=NOVA / "docs/validation/diagnostic-uefi-5l"
    )
    p.add_argument("--lecteur-p9", type=Path)
    a = p.parse_args()
    sys.path.extend([str(NOVA / "tests"), str(NOVA / "scripts")])
    for row in (a.dossier / "SHA256SUMS").read_text().splitlines():
        h, f = row.split()
        check(sha256((a.dossier / f).read_bytes()).hexdigest() == h, f)
    with tarfile.open(a.dossier / "observations.tar.gz") as t:
        summary = {}
        for nom in EFI + DISQUES:

            def lire(f):
                return t.extractfile(nom + "/" + f).read()

            summary[nom] = (
                transition(nom, lire)
                if nom in EFI
                else stockage(nom, lire, a.lecteur_p9)
            )
        check(len({summary[n]["epoque"] for n in EFI}) == 4, "Époques dupliquées")

        def pilote(f):
            return t.extractfile("concurrence/" + f).read()

        d = json.loads(pilote("apic.json"))
        tr = pilote("trace.log")
        inter = pilote("intervalle.log")
        verdict = juger(tr, inter, d)
        choix = json.loads(pilote("selection.json"))
        obs = json.loads(pilote("observation.json"))
        i = ("SecureBoot", "SetupMode", "AuditMode").index(choix["variable"]) * 5
        commande, retour, reponse = obs["evenements"][i + 2 : i + 5]
        smi(inter, commande["offset"], retour["offset"], obs["controle"])
        check(
            verdict == obs["verdict"]
            and verdict["eoi_attribues"]
            and not verdict["intervalle_admis"],
            "Attribution EOI concurrent",
        )
        intervalle_appel = inter[commande["offset"] : reponse["offset"]]
        check(
            re.search(
                rb"SMM: after RSM\n(?:(?!memory_region_ops_write|SMM:).)*?apic_local_deliver vector 0 delivery mode 0\nServicing hardware INT=0x20",
                intervalle_appel,
                re.S,
            ),
            "Minuterie après RSM",
        )
    spec = importlib.util.spec_from_file_location(
        "verification_5k", NOVA / "boot/diagnostic-5k/verifier.py"
    )
    ancien = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ancien)
    with tarfile.open(a.dossier / "observations.tar.gz") as t:

        def temoin(f):
            return t.extractfile("temoin/" + f).read()

        ancien.verifier_duo("duo-retrait", temoin, a.lecteur_p9)
        s = construction("temoin", temoin)["stockage"]
        actif = json.loads(t.extractfile("actif-wait/campagne.json").read())
        check(
            actif["elf"] == json.loads(temoin("campagne.json"))["elf"],
            "Identité du témoin apparié",
        )
        check(s["E_S_apres_WAIT_NIC"] == 31, "Continuité du témoin")
        summary["temoin"] = dict(
            commandes_apres_wait=31,
            invalidation_us=chronometrie(temoin("serie.log"), True),
        )
    with tarfile.open(a.dossier / "regression-5k.tar.gz") as t:
        for nom in ancien.EDU + ancien.DUO:

            def piece(f):
                return t.extractfile(nom + "/" + f).read()

            if nom in ancien.EDU:
                ancien.verifier_edu(nom, piece)
            else:
                ancien.verifier_duo(nom, piece, a.lecteur_p9)
    print("Contre-lecture des dix nouveaux démarrages 5K réussie.")
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
