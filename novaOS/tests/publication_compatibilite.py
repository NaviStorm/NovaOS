"""Présente PC1 au noyau installé v0.15.0 et au service P9 livrés, sans écrire d'image P9."""
import argparse
import json
from pathlib import Path
import shutil
import struct
import tempfile
import uuid
import zlib

from installation_gpt import attendre, compteurs, machine, verifier_gpt
from publication_conditionnelle import MIO, empreinte, lanceurs, pc

TYPE_PC1 = uuid.UUID("6d427fc3-1c6d-4fd0-a615-c8ac98347d2c").bytes_le


def hybride(cible, original, secteur, type_pc1, ids):
    # On copie sur un fichier neuf le démarrage EFI et les deux GPT. Aucun bloc
    # du LV P9 source n'est copié dans l'étendue de test.
    table = verifier_gpt(original, secteur, ids)
    debut = table["p9"] * secteur
    fin = (table["fin_p9"] + 1) * secteur
    with original.open("rb") as src, cible.open("xb+") as dst:
        taille = original.stat().st_size
        src.seek(2*secteur + 128 + 40)
        limite = (struct.unpack("<Q",src.read(8))[0]+1)*secteur
        assert limite == fin
        src.seek(0)
        dst.truncate(taille)
        restants = debut
        while restants:
            morceau = src.read(min(restants,MIO))
            assert morceau
            dst.write(morceau)
            restants -= len(morceau)
        dst.write(pc.octets(pc.NOUVEAU))
        src.seek(limite)
        dst.seek(limite)
        shutil.copyfileobj(src,dst)
        if type_pc1:
            for adresse in (secteur, taille-secteur):
                dst.seek(adresse)
                h = bytearray(dst.read(secteur))
                a = struct.unpack_from("<Q",h,72)[0]*secteur
                dst.seek(a)
                entrees = bytearray(dst.read(16384))
                entrees[128:144] = TYPE_PC1
                # N=256 pour la partition de type PC1.
                struct.pack_into("<Q",entrees,168,table["p9"]+MIO//secteur-1)
                struct.pack_into("<I",h,88,zlib.crc32(entrees))
                struct.pack_into("<I",h,16,0)
                struct.pack_into("<I",h,16,zlib.crc32(h[:92]))
                dst.seek(a)
                dst.write(entrees)
                dst.seek(adresse)
                dst.write(h)
    return table


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("anciens",type=Path)
    p.add_argument("firmware",type=Path)
    p.add_argument("sortie",type=Path)
    a = p.parse_args()
    out = a.sortie.resolve()
    out.mkdir()
    cas = []
    with tempfile.TemporaryDirectory(prefix="pc1-gpt-",dir="/tmp") as tmp:
        tmp = Path(tmp)
        lanceurs(tmp)
        for s in (512,4096):
            original = a.anciens/f"systeme-{s}/installe.raw"
            h_source = empreinte(original)
            ids = (original.parent/"guids.bin").read_bytes()
            for prive in (True,False):
                nom = f"{s}-" + ("type-pc1" if prive else "etiquette-p9")
                d = out/nom
                d.mkdir()
                (d/"tpm").mkdir()
                for banque in ("code.fd","vars.fd"):
                    shutil.copyfile(a.firmware/banque,d/banque)
                cible = d/"jetable.raw"
                hybride(cible,original,s,prive,ids)
                avant = empreinte(cible)
                def action(vm, canal, serie, trace):
                    if prive:
                        texte = attendre(vm,serie,b"SYSTEME bilan Err(Geometry)",timeout=75)
                        assert b"nova> " not in texte
                    else:
                        # Le chemin distribué peut refuser à l'initialisation ou au premier accès.
                        texte = attendre(vm,serie,b"BLOC bilan mode=41",timeout=75)
                        assert b"nova> " not in texte
                        texte = attendre(vm,serie,b"SYSTEME bilan Err(",timeout=20)
                    stats = compteurs(vm)["cible"]
                    assert stats["wr_bytes"] == stats["flush_operations"] == 0
                    return dict(lectures=stats["rd_operations"],ecritures=stats["wr_operations"],
                                flush=stats["flush_operations"])
                r = machine(tmp,d,nom,None,cible,s,None,action)
                assert empreinte(cible) == avant and empreinte(original) == h_source
                cas.append(dict(profil=nom,sha256_avant_apres=avant,source=h_source,**r))
                print(nom,r,flush=True)
    (out/"resultats.json").write_text(json.dumps(cas,indent=2)+"\n")


if __name__ == "__main__":
    main()
