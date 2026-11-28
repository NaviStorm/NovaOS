"""Qualification de la réception par le premier EFI. Les supports sont des fichiers synthétiques."""

import argparse
import gzip
import json
import struct
import sys
import uuid
from pathlib import Path

NOVA = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(NOVA / "boot/diagnostic-5q"))
import banc_usb as q
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from image import pe_info
from politique import FIRMWARE, canon, empreinte, exiger
from scenarios import CAS, CHAMPS
from supports import OWNER, Edk2VarStore, comparer_magasin


def ecrire(p, obj):
    p.write_text(json.dumps(obj, sort_keys=True, indent=2) + "\n")


def variante(body, case, key):
    b = bytearray(body[:-64])
    co = json.loads(b[288:])
    if case == "epoque":
        b[16] ^= 1
    elif case == "mapping":
        co["ressources"][0]["droits"] = 3
    elif case == "en-cours":
        co["transactions"]["achevees"] -= 1
    elif case == "substitution-contrat":
        for i, field in enumerate(("soumises", "achevees")):
            co["transactions"][field] += 1
            struct.pack_into("<Q", b, 160 + 8 * (4 + i), co["transactions"][field])
    elif case == "origine":
        co["source"][2][1] += 1
    elif case == "identite-pe":
        b[64] ^= 1
    if case in ("mapping", "en-cours", "origine", "substitution-contrat"):
        payload = canon(co)
        b[128:160] = bytes.fromhex(empreinte(payload))
        b[288:] = payload
    return bytes(b) + key.sign(bytes(b))


def preparer(ref, pe, out):
    out.mkdir(mode=0o700)
    original = out / "original.fd"
    copie = out / "actif.fd"
    exiger(
        empreinte(ref)
        == "bf41a49e08223f8e0a0192371d8fb79602faf56e4c0a58a5ad79624a54f2ed8f",
        "Référence 5O",
    )
    exiger(
        empreinte(pe)
        == "7f80bf3c42c89162d67fb229b9080e9706c0f7f6a3574978051c1c0c555477a5",
        "PE hors de l’autorisation 5R",
    )
    original.write_bytes(ref)
    edk = Edk2VarStore(str(original))
    vs = edk.get_varlist()
    h = pe_info(pe)["authenticode"]
    exiger(
        vs["db"].sigdb.add_hash(uuid.UUID(OWNER), bytes.fromhex(h)),
        "Empreinte déjà présente",
    )
    vs["db"].data = bytes(vs["db"].sigdb)
    edk.write_varstore(str(copie), vs)
    diff = comparer_magasin(original, copie, [h])
    exiger(original.read_bytes() == ref, "Original modifié")
    ecrire(out / "diff.json", diff)


class Profil:
    def __init__(self, pe, mag, out):
        self.pe = pe
        self.mag = mag
        self.out = out

    def magasin(self, reference):
        original = self.mag / "original.fd"
        copie = self.mag / "actif.fd"
        exiger(original.read_bytes() == reference, "Référence OVMF")
        comparer_magasin(original, copie, [pe_info(self.pe)["authenticode"]])
        return copie.read_bytes()

    def source(self, cas):
        return q.chargeur() if cas == "substitue" else self.pe

    def livrer(self, case, envelope, path, contract, key):
        e = bytes.fromhex(contract["epoque"])
        pe = bytes.fromhex(contract["pe"])
        mode = contract["secure_boot"]
        geometry = list(contract["surface"])
        if case == "surface":
            geometry[0] += 4096
        body = (
            b"NOVA5R-DISPLAY1!"
            + e
            + pe
            + struct.pack("<QQIIII", *geometry)
            + bytes([int(mode == "actif"), 1, 1, 0, 0, 0, 0, 0])
            + bytes.fromhex(empreinte(envelope))
            + bytes.fromhex(self.media_sha)
        )
        lease = body + key.sign(body)
        delivered = envelope
        if case == "absent":
            delivered = b""
        elif case == "altere":
            delivered = envelope[:-1] + bytes([envelope[-1] ^ 1])
        elif case == "autorite":
            delivered = envelope[:-64] + Ed25519PrivateKey.generate().sign(
                envelope[:-64]
            )
        elif case == "reutilise":
            delivered = (self.out / (mode + "-nominal") / "transfert.bin").read_bytes()
            exiger(delivered[16:32] != e, "Époque réutilisée dans le banc")
        elif case in (
            "epoque",
            "mapping",
            "origine",
            "en-cours",
            "identite-pe",
            "substitution-contrat",
        ):
            delivered = variante(envelope, case, key)
        fixtures = NOVA / "boot/diagnostic-5e/vecteurs"
        md = (fixtures / "ioapic.toml").read_bytes()
        sig = (fixtures / "ioapic.sig").read_bytes()
        elf = (NOVA / "libs/catalogue-pilotes/vecteurs/x86_64.pkg01").read_bytes()
        if case == "manifeste":
            sig = sig[:-1] + bytes([sig[-1] ^ 1])
        if case == "elf":
            elf = elf[:-1] + bytes([elf[-1] ^ 1])
        block = lease + b"".join(
            struct.pack("<I", len(v)) + v for v in (delivered, path, md, sig, elf)
        )
        Path("livraison.bin").write_bytes(block)
        return block

    def photographier(self, mon, suffixe, original, livraison, phase):
        # Relecture indépendante de la RAM invitée, VM arrêtée à RIP initial ou HLT.
        lu = q.ram(mon, suffixe, len(original)) + q.ram(
            mon, suffixe + 8192, len(livraison)
        )
        exiger(lu == original + livraison, "Remise modifiée en RAM invitée")
        Path(phase + "-remise.bin").write_bytes(lu)

    def rendu(self, case):
        return case != "surface"

    def decision(self, case):
        return (
            "diagnostic-acheve"
            if CAS[case] == 0
            else ("refus-affiche" if self.rendu(case) else "refus-sans-surface")
        )

    def rapport(self, case, verdict, mon, base, size, contract):
        addr = verdict["adresse"] + 104
        exiger(base <= addr and addr + 128 <= base + size, "Objet de résultat hors PE")
        raw = q.ram(mon, addr, 128)
        Path("details.bin").write_bytes(raw)
        r = dict(zip(CHAMPS, struct.unpack_from("<12Q", raw)))
        r["plan"] = raw[96:].hex()
        r.update(
            schema="novaos.diagnostic/5r",
            epoque=verdict["epoque"],
            pe_sha256=verdict["image"],
            contrat_sha256=empreinte(Path("transfert.bin").read_bytes()),
            octets_arene=verdict["arena"],
            allocations_vivantes=verdict["vivants"],
            statut=verdict["statut"],
            secure_boot=(
                "actif"
                if verdict["confiance"] == 1
                else "inactif"
                if verdict["confiance"] == 0
                else "indetermine"
            ),
            origine="usb-authentifiee" if r["usb"] else "indeterminee",
            activation_autorisee=False,
        )
        r["pilotes_compatibles"] = ["org.novaos.lab.ioapic"] if r["compatibles"] else []
        r["conflits_selection"] = (
            ["org.novaos.lab.hpet-a", "org.novaos.lab.hpet-b"] if r["conflits"] else []
        )
        ecrire(Path("rapport.json"), r)
        Path("journal.txt").write_text(
            f"NovaOS 5R — statut {r['statut']}, origine {r['origine']}, Secure Boot {r['secure_boot']}\n{r['pci']} fonctions PCI, {r['pages']} pages USB, {r['compatibles']} sélection, {r['conflits']} conflit, activation interdite.\n"
        )
        exiger(
            verdict["statut"] == CAS[case],
            f"Verdict {verdict['statut']} au lieu de {CAS[case]} pour {case}",
        )
        exiger(not r["activation"] and not verdict["vivants"], "Activation ou fuite")
        if CAS[case] == 0:
            expected = empreinte(
                (NOVA / "boot/diagnostic-5e/vecteurs/ioapic.toml").read_bytes()
            )
            exiger(
                r["usb"] == 1
                and r["rid"] == contract["rid"]
                and r["port"] == contract["port"]
                and r["pages"] == sum(l["pages"] for l in contract["ressources"])
                and 0 < r["pci"] <= 64
                and r["ioapic"] == r["compatibles"] == 1
                and r["drhd"] == 1
                and r["tables"]
                and r["plan"] == expected,
                "Inventaire ou plan",
            )
        return CAS[case]


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="cmd", required=True)
    a = sub.add_parser("magasin")
    a.add_argument("pe", type=Path)
    a.add_argument("sortie", type=Path)
    a = sub.add_parser("qualifier")
    for n in ("sortie", "build5j", "build5q", "pe", "magasin", "cle"):
        a.add_argument(n, type=Path)
    a.add_argument("--mode", action="append", choices=["actif", "inactif"])
    a.add_argument("--cas", action="append", choices=list(CAS))
    args = p.parse_args()
    if args.cmd == "magasin":
        preparer(
            gzip.decompress((q.PREUVES / "magasin-ovmf-test.fd.gz").read_bytes()),
            args.pe.read_bytes(),
            args.sortie,
        )
        return
    root = args.sortie.resolve()
    root.mkdir(mode=0o700)
    j = args.build5j.resolve()
    b = args.build5q.resolve()
    exe = q.k.exact(
        j / "modele5j/build/qemu-system-x86_64",
        "085ed32ca433172258c779258f435cb44388e5e6f64d9d8fd816c1fd4f27791e",
    )
    fw = q.k.exact(b / "edk/Build/OvmfX64/RELEASE_CLANGDWARF/FV/OVMF_CODE.fd", FIRMWARE)
    key = q.cle_autorisee(args.cle)
    profil = Profil(args.pe.read_bytes(), args.magasin.resolve(), root)
    for name, data in (
        ("usb", profil.pe),
        ("clone", profil.pe),
        ("substitue", q.chargeur()),
    ):
        q.creer(root / (name + ".img"), 68 * q.MIO, data)
    with q.k.qualification(NOVA.parent.parent, {}):
        for mode in args.mode or ("inactif", "actif"):
            for cas in args.cas or CAS:
                if cas == "hors-db" and mode == "inactif":
                    continue
                dest = root / (mode + "-" + cas)
                media = root / ("substitue.img" if cas == "substitue" else "usb.img")
                profil.media_sha = empreinte(media.read_bytes())
                q.run(
                    dest,
                    exe,
                    fw,
                    j,
                    media,
                    key,
                    mode,
                    cas,
                    b / "controle.dylib",
                    profil,
                )
                if mode == "actif":
                    before = profil.mag / (
                        "original.fd" if cas == "hors-db" else "actif.fd"
                    )
                    vs = Edk2VarStore(str(before)).get_varlist()
                    after = Edk2VarStore(str(dest / "variables.fd")).get_varlist()
                    ok = {
                        n: all(
                            getattr(vs[n], f) == getattr(after[n], f)
                            for f in ("data", "guid", "attr", "count", "pkidx", "time")
                        )
                        for n in ("PK", "KEK", "db", "dbx")
                    }
                    exiger(all(ok.values()), "Politique Secure Boot après arrêt")
                    ecrire(dest / "variables.json", ok)
                print(
                    mode,
                    cas,
                    json.loads((dest / "bilan.json").read_bytes())["decision"],
                    flush=True,
                )


if __name__ == "__main__":
    main()
