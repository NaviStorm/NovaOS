"""Oracles en lecture seule du banc NVMe. Une trace matérielle et les deux ancres P9."""
import hashlib
import re
import subprocess

from vtd_edu import require

BLOCK = 4096
RAW_BYTES = 4 << 20
P9_BLOCKS = RAW_BYTES // BLOCK - 1
OLD = b"ancien"
NEW = bytes((i * 73 & 255) ^ (i >> 5 & 255) for i in range(4231))
FILE = "/sortie"
GUARD = bytes((i * 11 + (i >> 8) + 67) & 255 for i in range(BLOCK))


def fingerprint(data):
    return hashlib.sha256(data).hexdigest()


def virgin(path):
    with path.open("xb") as fd:
        fd.seek(RAW_BYTES - BLOCK)
        fd.write(GUARD)


def view(image):
    with image.open("rb") as fd:
        data = fd.read(RAW_BYTES - BLOCK)
        require(len(data) == RAW_BYTES - BLOCK and fd.read(BLOCK) == GUARD and not fd.read(1),
                "Étendue RAW ou dernière page hors bail altérée")
        return data


def inspect(reader, image, sector):
    data = view(image)

    def run(*argv):
        proc = subprocess.run([reader, argv[0], str(P9_BLOCKS), str(sector), *argv[1:]],
                              input=data, capture_output=True)
        require(proc.returncode == 0, f"Lecteur P9 {argv}: {proc.stderr[-512:]!r}")
        return proc.stdout

    heads = re.findall(r"slot=([01]) génération=(\d+) blocs=(\d+) FS=(\w+) LV=(\w+) manifeste=(\d+) sha256=([0-9a-f]{64})".encode(), run("inspect"))
    require(len(heads) == 2 and {int(h[0]) for h in heads} == {0, 1}, "Les deux ancres doivent être valides")
    fs, lv = (s.hex() for s in (b"NOVA0190-Q35-P9F", b"NOVA0190-Q35-P9V"))
    require(all(int(n) == P9_BLOCKS and f.decode() == fs and l.decode() == lv
                for _, _, n, f, l, _, _ in heads), "Identité ou capacité P9 différente du volume de test")
    run("verify", fs, lv)
    slots = {int(i): {"generation": int(g), "manifeste_bloc": int(m), "sha256": h.decode()}
             for i, g, _, _, _, m, h in heads}
    latest = max(slots, key=lambda i: slots[i]["generation"])
    require(slots[latest]["generation"] == slots[1-latest]["generation"] + 1,
            "Les deux générations P9 doivent se suivre")
    content = run("salvage", fs, lv, str(latest), FILE)
    require(content in (OLD, NEW), "Fichier absent, tronqué ou hybride après remontage")
    return {"etat": "ancien" if content == OLD else "nouveau", "slot": latest,
            "generation": slots[latest]["generation"], "ancres": slots,
            "fichier": FILE, "octets": len(content), "sha256_fichier": fingerprint(content),
            "sha256_p9": fingerprint(data), "sha256_garde": fingerprint(GUARD)}


def verify_transition(old, after, allowed):
    require(after["etat"] in allowed, f"État {after['etat']} incompatible avec {allowed}")
    require(after["generation"] == old["generation"] + (after["etat"] == "nouveau"),
            "Recul ou publication supplémentaire après coupure")
    saved = old["slot"]
    require(old["ancres"][saved] == after["ancres"][saved], "L'ancienne génération confirmée a changé")


def unconfirmed_write(seed, image, ops, sector):
    permitted = [(o["lba"]*sector, (o["lba"]+o["blocs"])*sector)
                 for o in ops if o["op"] == "WRITE"]
    require(permitted and all(2*BLOCK <= lo < hi <= P9_BLOCKS*BLOCK for lo,hi in permitted),
            "Une écriture du profil de faute cible une ancre ou la garde")
    changed = {}
    with seed.open("rb") as before, image.open("rb") as after:
        for pos in range(0, RAW_BYTES, BLOCK):
            old, new = before.read(BLOCK), after.read(BLOCK)
            require(len(old) == len(new) == BLOCK, "Image de taille imprévue")
            if old == new:
                continue
            delta = [pos+i for i,(a,b) in enumerate(zip(old,new)) if a != b]
            require(all(any(lo <= offset < hi for lo,hi in permitted) for offset in delta),
                    "Modification non expliquée par une commande WRITE soumise")
            changed[pos//BLOCK] = len(delta)
        require(not before.read(1) and not after.read(1), "Fin d'image inattendue")
    return {"octets_changes":sum(changed.values()), "blocs_p9_changes":sorted(changed)}


def io_trace(path, sector, epochs=1):
    """Rapprocher CID, commande, NSID, plage et CQ de l'unique paire d'E/S."""
    lines = path.read_text(errors="replace").splitlines()
    if epochs == 1:
        return decode_io(lines, sector)
    starts = [i for i,line in enumerate(lines) if line.startswith("pci_nvme_admin_cmd cid 1 sqid 0 opc 0x6 ")]
    require(len(starts)==epochs, "Nombre d'incarnations du contrôleur")
    require(not any(row.startswith("pci_nvme_io_cmd ") for row in lines[:starts[0]]),
            "Commande d'E/S antérieure à l'identification initiale")
    result = []
    for epoch, (start,end) in enumerate(zip(starts,starts[1:]+[len(lines)]), 1):
        phase = lines[start:end]
        reset = [i for i,row in enumerate(phase) if row.endswith("config controller config=0x0")]
        require(len(reset)==1 and not any(row.startswith(("pci_nvme_io_cmd ","pci_nvme_enqueue_req_completion "))
                                         for row in phase[reset[0]+1:]),
                "Frontière matérielle absente ou commande/CQE après réinitialisation")
        result.extend(op | {"epoque":epoch} for op in decode_io(phase,sector))
    return result


def decode_io(lines, sector):
    ops = {}
    for line in lines:
        if line.startswith("pci_nvme_io_cmd "):
            m = re.match(r"pci_nvme_io_cmd cid (\d+) nsid 0x([0-9a-f]+) sqid (\d+) opc 0x([0-9a-f]+)", line)
            require(m, line)
            cid, nsid, sq, opcode = (int(m[1]), int(m[2], 16), int(m[3]), int(m[4], 16))
            require(nsid == 1 and sq == 1 and opcode in (0, 1, 2) and cid not in ops,
                    f"Admission ou réemploi inattendu {line}")
            ops[cid] = {"cid": cid, "op": ("FLUSH", "WRITE", "READ")[opcode]}
        elif line.startswith(("pci_nvme_read ", "pci_nvme_write ")):
            m = re.match(r"pci_nvme_(read|write) cid (\d+)(?: opname '[^']+')? nsid 1 nlb (\d+) count (\d+) lba 0x([0-9a-f]+)", line)
            require(m and int(m[2]) in ops, f"Descripteur inattendu {line}")
            cid, n, size, lba = int(m[2]), int(m[3]), int(m[4]), int(m[5], 16)
            require(ops[cid]["op"] == m[1].upper() and 0 < size == n*sector <= 4096
                    and (lba+n)*sector <= P9_BLOCKS*BLOCK, f"Taille ou plage hors bail {line}")
            ops[cid].update(lba=lba, blocs=n)
        elif line.startswith("pci_nvme_enqueue_req_completion ") and " cqid 1 " in line:
            m = re.match(r"pci_nvme_enqueue_req_completion cid (\d+) cqid 1 .+ status (0x[0-9a-f]+)", line)
            require(m and int(m[1]) in ops and "cqe" not in ops[int(m[1])], f"CQE orpheline {line}")
            ops[int(m[1])]["cqe"] = int(m[2], 16)
    for op in ops.values():
        require(op["op"] == "FLUSH" or "lba" in op, f"Commande sans taille dans la trace {op}")
    return list(ops.values())


def chronology(ops, old, sector, cut=None):
    """Vérifier les deux barrières et le seul bloc d'ancre hors de toute classification par le client."""
    writes = [o for o in ops if o["op"] != "READ"]
    first = next((i for i, o in enumerate(writes) if o["op"] == "WRITE"),len(writes))
    writes = writes[first:]  # un FLUSH sans écriture de l'ouverture n'est pas F1.
    target = (1-old["slot"])*BLOCK
    anchors = [i for i,o in enumerate(writes) if o["op"] == "WRITE" and o["lba"]*sector < 2*BLOCK]
    barriers = [i for i,o in enumerate(writes) if o["op"] == "FLUSH"]
    if anchors:
        require(anchors == list(range(min(anchors), max(anchors)+1)), "Écriture parasite pendant la publication")
    if cut == "before-write":
        require(not writes, "Commande d'écriture avant autorisation applicative")
    elif cut in ("during-write","after-write"):
        require(writes and not anchors and not barriers, "Ordre de préparation avant F1")
    elif cut in ("during-f1","after-f1"):
        require(len(barriers) == 1 and not anchors and barriers[0] > 0, "F1 doit suivre la préparation CoW")
    else:
        require(barriers and anchors and min(anchors) == barriers[0]+1, "Publication avant F1 ou ordre des métadonnées")
        ranges = [(writes[i]["lba"]*sector,writes[i]["blocs"]*sector) for i in anchors]
        end = target
        for lba,length in ranges:
            require(lba == end and end+length <= target+BLOCK, "Écriture de l'autre ancre ou chevauchement")
            end += length
        if cut == "during-publish":
            require(len(barriers) == 1, "F2 avant la fin de la publication")
        else:
            require(end == target+BLOCK and len(barriers) == 2 and barriers[-1] == max(anchors)+1,
                    "F2 doit suivre immédiatement la publication complète")
    during = cut and cut.startswith("during-")
    if during:
        require(writes and "cqe" not in writes[-1], "La commande suspendue a déjà été acquittée")
    require(all(o.get("cqe") == 0 for o in (writes[:-1] if during else writes)),
            "CQE manquante ou en erreur avant la frontière")
    return {"premier_cid": writes[0]["cid"] if writes else None, "write": len(writes)-len(barriers),
            "flush_cid": [writes[i]["cid"] for i in barriers],
            "ancre_cid": [writes[i]["cid"] for i in anchors],
            "derniere": writes[-1] if writes else None}
