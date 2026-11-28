"""Observer l’image 6A exacte sous Q35. Le banc 5Z conserve la propriété des VM."""
import argparse
import json
from pages_video import controler
from hashlib import sha256
from pathlib import Path
import re
import runpy
import struct
import sys
import time

from pixels import TEXTE, png, verifier
LIVE = Path(__file__).resolve().parents[1] / "live-5z"
sys.path.insert(0, str(LIVE))
from image import admettre, construire
from relire import nominal
b = runpy.run_path(str(LIVE / "banc.py"))


def photo(q, nom):
    q.call("screendump", filename=nom + ".ppm")
    raw = Path(nom + ".ppm").read_bytes()
    Path(nom + ".png").write_bytes(png(raw))
    return raw


def journal(d, syms):
    return int.from_bytes(b["gmem"](d, syms["NOVA6A_JOURNAL"], 8), "little")


def evenements(d, syms, attendus):
    ligne = list(struct.unpack("<8Q", b["gmem"](d, syms["NOVA6A_EVENEMENTS"], 64)))
    if ligne != attendus + [0] * (8 - len(attendus)):
        raise ValueError(f"Historique RAM discordant {ligne}")
    return attendus


def stable(q):
    # Relancer les horloges de la machine avant de comparer les deux états terminaux.
    q.call("cont")
    time.sleep(.25)
    q.call("stop")
    etats = {}
    for cpu in q.call("query-cpus-fast"):
        n = cpu["cpu-index"]
        b["hmp"](q, f"cpu {n}")
        r = b["hmp"](q, "info registers")
        if "HLT=1" not in r or int(re.search(r"RFL=([0-9a-fA-F]+)", r)[1], 16) & 0x200:
            raise ValueError(f"vCPU {n} hors arrêt local masqué")
        etats[n] = r
    b["hmp"](q, "cpu 0")
    return etats


def observer(etape, q, d, syms):
    data = photo(q, etape)
    valeur = journal(d, syms)
    cible = {"entree": 0, "memoire": 3, "premiere-tache": 3, "ordonnanceur": 7, "smp": 15, "terminal": 31}[etape]
    if valeur != cible:
        raise ValueError(f"Journal RAM {etape} {valeur:#x}, attendu {cible:#x}")
    cpu = len(q.call("query-cpus-fast"))
    textes = TEXTE.copy()
    textes[4] = f"4  SMP verifie - {cpu:02} CPU"
    sequence = [1, 2, 4, 8 | cpu << 16, 16][:{0: 0, 3: 2, 7: 3, 15: 4, 31: 5}[valeur]]
    mesure = {"journal": valeur, "evenements": evenements(d, syms, sequence), "sha256_ppm": sha256(data).hexdigest()}
    if etape in ("entree", "memoire"):
        pages = controler(q, d, syms, etape, b["gmem"], b["hmp"])
        mesure["surface"] = pages
        if etape == "memoire":
            avant = json.loads(Path("entree.json").read_text())["surface"]
            if (avant["cr3"] == pages["cr3"]
                    or any(avant[k] != pages[k] for k in ("surface_va", "surface_pa", "pages", "pas"))):
                raise ValueError("Continuité de la surface au changement de racine")
    if etape == "entree":
        try:
            verifier(data, TEXTE[:2])
        except ValueError:
            pass
        else:
            raise ValueError("Le panneau existe déjà à nova_start")
    else:
        mesure["pixels"] = verifier(data, textes[:{3: 3, 7: 4, 15: 5, 31: 6}[valeur]])
    if etape == "terminal":
        mesure["registres_apres_reprise"] = stable(q)
        if data != photo(q, "terminal-stable"):
            raise ValueError("Verdict instable")
    b["enregistrer"](etape + ".json", mesure)


def pano_terminal():
    limite = time.monotonic() + 40
    while time.monotonic() < limite:
        m = re.search(rb"NOVA6A journal=(0x[0-9a-f]+) refus=R(\d\d)", Path("serie.log").read_bytes())
        if m:
            return int(m[1], 16), int(m[2])
        time.sleep(.05)
    raise ValueError("Verdict terminal absent")


def panne(cas, out, stage, image, fw, var, ram, cpus):
    # Seuls les champs identifiés du relais synthétique sont substitués.
    # L'image disque reste ouverte en lecture seule, y compris pendant ces essais.
    video = cas != "sans-vga"
    opts = image, fw, var, ram, cpus, video
    with b["machine"](out / "localisation", *opts) as q:
        q.call("cont")
        s = b["attendre"](b" | lancement Limine")
        entree = int(re.search(rb"NOVA5Z EFI entree=([0-9a-f]+)", s)[1], 16)
    syms = b["symboles"](stage / "NOVAOS.ELF",
                         ("NOVA6A_JOURNAL", "NOVA6A_EVENEMENTS", "novaos::boot::FRAMEBUFFER", "novaos::boot::IMAGE",
                          "novaos::boot::CMDLINE", "novaos::boot::MP"))
    with b["machine"](out / "mesure", *opts) as q:
        d = b["Debug"]()
        try:
            b["atteindre"](d, entree)
            avant = q.stats()
            canari = b["digest"](Path("interne.raw"))
            for event in b["TRACE"]:
                q.call("trace-event-set-state", name=event, enable=True)
            b["atteindre"](d, syms["nova_start"])
            initial = photo(q, "avant") if video else None
            changement = None
            if cas != "sans-vga":
                nom = {"relais": "IMAGE", "memoire": "IMAGE", "profil": "CMDLINE",
                       "topologie": "MP", "sans-surface": "FRAMEBUFFER"}[cas]
                adresse = syms["novaos::boot::" + nom] + 40
                orig = int.from_bytes(b["gmem"](d, adresse, 8), "little")
                if not orig:
                    raise ValueError("Relais initial absent")
                if cas == "memoire":
                    adresse = orig + 8  # base physique de la réponse ExecutableAddress
                    orig = int.from_bytes(b["gmem"](d, adresse, 8), "little")
                    valeur = orig | 1
                elif cas == "topologie":
                    adresse = orig + 16  # nombre de processeurs du relais MP x86_64
                    orig = int.from_bytes(b["gmem"](d, adresse, 8), "little")
                    if orig != cpus:
                        raise ValueError("Topologie initiale inattendue")
                    valeur = 0
                else:
                    valeur = 0
                raw = struct.pack("<Q", valeur)
                if d.command(f"M{adresse:x},8:{raw.hex()}") != "OK" or b["gmem"](d, adresse, 8) != raw:
                    raise ValueError("Description de panne non établie")
                changement = dict(champ=nom, adresse=adresse, avant=orig, apres=valeur)
            q.call("cont")
            vu, code = pano_terminal()
            q.call("stop")
            etats = stable(q)
            attendu = {"relais": (0x10011, 1), "memoire": (0x20011, 2), "profil": (0x30011, 3),
                       "topologie": (0x40011, 4),
                       "sans-surface": (0x6011f, 6), "sans-vga": (0x6011f, 6)}[cas]
            if (vu, code) != attendu or journal(d, syms) != vu:
                raise ValueError(f"Verdict de panne {vu:#x} R{code:02}")
            sequence = ([256, 1, 2, 4, 8 | cpus << 16] if code == 6 else [1]) + [0x200 | code, 16]
            evenements(d, syms, sequence)
            if video:
                fin = photo(q, "refus")
                if cas == "sans-surface":
                    if initial != fin:
                        raise ValueError("Écriture sans surface")
                else:
                    msg = {1: "REFUS R01 - relais memoire", 2: "REFUS R02 - memoire noyau",
                           3: "REFUS R03 - profil de boot", 4: "REFUS R04 - avant ordonnanceur"}[code]
                    verifier(fin, [*TEXTE[:2], "", "", "", msg])
                stable(q)
                if fin != photo(q, "refus-stable"):
                    raise ValueError("Pixels du refus instables")
            apres = q.stats()
            if avant != apres or b["digest"](Path("interne.raw")) != canari:
                raise ValueError("E/S ou altération du SATA durant le refus")
            b["enregistrer"]("panne.json", dict(cas=cas, injection=changement, journal=vu, evenements=sequence, refus=code,
                                                etats=etats, sata_efi=avant, sata_fin=apres,
                                                canari=canari, image=b["digest"](image)))
        finally:
            d.sock.close()


def integrite(out, stage, reference, fw, var, ram, cpus):
    opts = fw, var, ram, cpus
    with b["machine"](out / "localisation", reference, *opts) as q:
        q.call("cont")
        s = b["attendre"](b" | lancement Limine")
        entree = int(re.search(rb"NOVA5Z EFI entree=([0-9a-f]+)", s)[1], 16)
    for role, motif in (("PLAN.BIN", "MANIFESTE"), ("LIMINE.EFI", "LIMINE"),
                        ("limine.conf", "CONFIG"), ("NOVAOS.ELF", "NOYAU")):
        image = out / (motif + ".img")
        construire(stage, image, defaut=role, octets=reference.stat().st_size)
        with b["machine"](out / motif, image, *opts) as q:
            d = b["Debug"]()
            try:
                b["atteindre"](d, entree)
                avant = q.stats()
                canari = b["digest"](Path("interne.raw"))
                q.call("cont")
                serie = b["attendre"](("NOVA5Z REFUS " + motif).encode())
                q.call("stop")
                etats = stable(q)
                apres = q.stats()
                if (avant != apres or canari != b["digest"](Path("interne.raw"))
                        or b"lancement Limine" in serie or b"NOVA6A" in serie):
                    raise ValueError("Refus incomplet ou E/S interne")
                b["enregistrer"]("refus.json", dict(motif=motif, sata_efi=avant, sata_fin=apres,
                                                    canari=canari, image=b["digest"](image), etats=etats))
            finally:
                d.sock.close()


def principal():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("build", type=Path)
    p.add_argument("sortie", type=Path)
    p.add_argument("--sha256", required=True)
    p.add_argument("--firmware", type=Path, required=True)
    p.add_argument("--variables", type=Path, required=True)
    p.add_argument("--ram", choices=[256, 384, 512], type=int, default=384)
    p.add_argument("--cpus", choices=[1, 2, 4], type=int, default=2)
    p.add_argument("--cas", choices=["nominal", "relais", "memoire", "profil", "topologie", "sans-surface", "sans-vga", "integrite"],
                   default="nominal")
    a = p.parse_args()
    build, out, fw, var = (x.resolve() for x in (a.build, a.sortie, a.firmware, a.variables))
    image = build / "novaos-live.img"
    if image.stat().st_size != 4005560320:
        raise ValueError("Capacité de la candidature A8")
    admettre(image, a.sha256)
    out.mkdir(parents=True, exist_ok=False)
    with b["qualification"](b["NOVA"].parents[1], {}):
        if a.cas == "nominal":
            b["essai"](out, build / "stage", fw, var, image, a.ram, a.cpus, True, observer)
            print(nominal(out / f"nominal-{a.ram}-{a.cpus}", a.sha256))
        elif a.cas == "integrite":
            integrite(out, build / "stage", image, fw, var, a.ram, a.cpus)
        else:
            panne(a.cas, out, build / "stage", image, fw, var, a.ram, a.cpus)


if __name__ == "__main__":
    principal()
