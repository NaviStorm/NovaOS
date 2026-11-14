"""Tester la collecte hors VM ; ajouter --qemu avec un profil coupe instrumenté."""
import argparse
import hashlib
import json
import re
import subprocess
import shutil
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from preuves_stockage import copie_reprise, dossier_essai, empreinte, figer_initiale, nettoyer_apres_validation


def lire(chemin):
    return json.loads(chemin.read_bytes())


class Conservation(unittest.TestCase):
    def test_interruption_dans_chaque_phase(self):
        with tempfile.TemporaryDirectory() as tmp:
            rapport = Path(tmp) / "rapport.json"
            for phase in ("avant", "pendant", "reprise"):
                with self.subTest(phase=phase):
                    with self.assertRaises(KeyboardInterrupt):
                        with dossier_essai(rapport, "aarch64", 512, phase) as dossier:
                            if phase != "avant":
                                image = dossier / "p9.raw"
                                image.write_bytes(b"avant")
                                figer_initiale(image)
                                image.write_bytes(b"apres")
                                if phase == "reprise":
                                    copie_reprise(image).write_bytes(b"montage")
                            raise KeyboardInterrupt
                    bilan = lire(dossier / "bilan.json")
                    self.assertEqual(bilan["resultat"], "echec")
                    self.assertEqual(lire(dossier / "debut.json")["injection"], phase)
                    if phase != "avant":
                        for nom, valeur in (("initiale.raw", b"avant"), ("p9.raw", b"apres")):
                            self.assertEqual(bilan["fichiers"][nom]["sha256"], hashlib.sha256(valeur).hexdigest())
                            self.assertEqual((dossier / nom).read_bytes(), valeur)
                            self.assertEqual((dossier / nom).stat().st_mode & 0o222, 0)
                    if phase == "reprise":
                        self.assertEqual((dossier / "reprise.raw").read_bytes(), b"montage")

    def test_statut_volume_et_sauvegarde(self):
        with tempfile.TemporaryDirectory() as tmp:
            with dossier_essai(Path(tmp) / "r.json", "x86_64", 512, "f1", conserver=True) as d:
                (d / "sauvegarde.serial").write_text(
                    "FS0170 transport tag=17 statut=12\nPHASE F1\nSURETE0170 tag=17\n"
                    "erreur d’E/S\nBLOC bilan mode=42 client=Exit(0) retrait=Ok(())\n")
                (d / "classique.serial").write_bytes(
                    b"BLOC bilan mode=24 service=Exit(0) client=Exit(26) retrait=Ok(())")
            traces = lire(d / "bilan.json")["series"]
            for nom in ("classique", "sauvegarde"):
                self.assertEqual([r["code"] for r in traces[nom+".serial"]["messages_publics_observes"]], [26])
            self.assertEqual(traces["sauvegarde.serial"]["traces_lignes"], {"ipc": 2, "stockage": 4})

    def test_ne_pas_ecraser_le_premier_passage(self):
        with tempfile.TemporaryDirectory() as tmp:
            rapport = Path(tmp) / "rapport.json"
            with self.assertRaisesRegex(ValueError, "injecté"):
                with dossier_essai(rapport, "x86_64", 4096, "vol-f2") as premier:
                    raise ValueError("injecté")
            bilan = (premier / "bilan.json").read_bytes()
            with dossier_essai(rapport, "x86_64", 4096, "vol-f2") as second:
                self.assertNotEqual(premier, second)
            self.assertFalse(second.exists())
            self.assertEqual((premier / "bilan.json").read_bytes(), bilan)

    def test_nettoyage_explicitement_confirme(self):
        with tempfile.TemporaryDirectory() as tmp:
            with dossier_essai(Path(tmp)/"rapport.json", "aarch64", 512, "f1", conserver=True) as d:
                (d / "p9.raw").write_bytes(b"synthese")
            sha = empreinte(d / "bilan.json")
            with self.assertRaises(ValueError):
                nettoyer_apres_validation(d, "0"*64)
            ajoute = d / "nouveau.txt"
            ajoute.write_text("preuve ajoutée")
            with self.assertRaises(ValueError):
                nettoyer_apres_validation(d, sha)
            ajoute.unlink()
            nettoyer_apres_validation(d, sha)
            self.assertFalse(d.exists())

    def test_conserver_exception_initiale_si_inventaire_echoue(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch("preuves_stockage.empreinte", side_effect=OSError("collecte")):
                with self.assertRaisesRegex(ValueError, "initiale") as e:
                    with dossier_essai(Path(tmp) / "r.json", "x86_64", 4096, "f1") as d:
                        raise ValueError("initiale")
            self.assertTrue(d.exists())
            self.assertTrue(any("collecte" in note for note in e.exception.__notes__))


def preuve_invitee(args):
    import coupure_cow_groupee as banc
    import vi_volumes as volumes
    from cli import Terminal

    volumes.HOST = args.oracle.resolve()
    args.dossier.mkdir(parents=True, exist_ok=True)
    # Attendre la frontière observée. Le disque et la série sortent par le chemin
    # d'échec ordinaire ; l'exception attendue est interceptée par le lanceur.
    class ArretBanc(RuntimeError):
        pass

    classique = Terminal.until

    def injecter(terminal, marque, *pos, **kwargs):
        result = classique(terminal, marque, *pos, **kwargs)
        if marque == "Saisir OUI puis Entrée : ".encode():
            raise ArretBanc("interruption avant confirmation du nom")
        return result

    try:
        with patch.object(Terminal, "until", injecter):
            with dossier_essai(args.dossier / "rapport.json", args.arch, args.secteur, "vol-f2",
                              parametres={"injection": "interruption avant confirmation du nom"}) as echec:
                banc.essai(args.arch, args.secteur, "vol-f2", echec, 4 << 20, True)
    except ArretBanc:
        bilan = lire(echec / "bilan.json")
    else:
        raise AssertionError("L'interruption prévue doit faire échouer le banc")

    assert bilan["resultat"] == "echec" and "ArretBanc" in bilan["exception"]
    for nom in ("initiale.raw", "p9.raw", "sauvegarde.vm.json", "sauvegarde.serial",
                "sauvegarde.ipc.log", "sauvegarde.stockage.log", "sauvegarde.virtio.log",
                "sauvegarde.qemu.log", "qmp.jsonl"):
        assert bilan["fichiers"][nom]["sha256"] == empreinte(echec / nom), nom
    assert not bilan["series"]["sauvegarde.serial"]["messages_publics_observes"]
    assert "exec qemu-system-" in (echec / "sauvegarde.qemu.log").read_text()
    assert b"virtio_blk_" in (echec / "sauvegarde.virtio.log").read_bytes()
    assert (echec / "sauvegarde.stockage.log").stat().st_size > 0
    cfg = lire(echec / "sauvegarde.vm.json")
    assert cfg["architecture"] == args.arch and cfg["octets_secteur"] == args.secteur
    assert cfg["iso_sha256"] == empreinte(echec / "sauvegarde.iso")
    assert any(json.loads(ligne).get("operation") == "query-blockstats"
               for ligne in (echec / "qmp.jsonl").read_text().splitlines())
    image = echec / "p9.raw"
    initiale = echec / "initiale.raw"
    with image.open("rb") as src:
        def h(action, *params, check=True):
            src.seek(0)
            return subprocess.run([volumes.HOST, action, "4096", str(args.secteur), *params],
                                  stdin=src, capture_output=True, check=check)
        slots = re.findall(rb"slot=(\d+).*?ration=(\d+).*?FS=(\w+) LV=(\w+)", h("inspect").stdout)
        assert len(slots) == 2
        slot, _, fs, lv = max(slots, key=lambda item: int(item[1]))
        h("verify", fs, lv)
        assert h("salvage", fs, lv, slot, "/f4194304").stdout == volumes.contents(4 << 20)
        absent = h("salvage", fs, lv, slot, "/coupure-vol-f2", check=False)
        assert absent.returncode and b"NotFound" in absent.stderr
    # Le profil cli-test peut publier une génération de préparation au démarrage.
    assert empreinte(initiale) == lire(echec / "images.json")["initiale_sha256"]
    # Relecture sur une nouvelle copie, hors du dossier de preuves figé au refus.
    with dossier_essai(args.dossier / "rapport.json", args.arch, args.secteur, "controle-reprise",
                      conserver=True, parametres={"incident": echec.name}) as reprise:
        r = reprise / "reprise.raw"
        shutil.copyfile(image, r)
        with Terminal(args.arch, args.secteur, r, "preuves-reprise", test=True,
                      journal=reprise / "reprise.serial", diagnostic=True) as terminal:
            terminal.until(b"nsh> ", 180)
            terminal.exit()
        assert empreinte(r) == empreinte(image)
    # Le dossier de l'incident reste intact même après le contrôle de reprise.
    print(echec, flush=True)
    return echec


if __name__ == "__main__":
    import sys
    if "--qemu" in sys.argv:
        p = argparse.ArgumentParser(description=__doc__)
        p.add_argument("--qemu", action="store_true")
        p.add_argument("--arch", choices=("aarch64", "x86_64"), required=True)
        p.add_argument("--secteur", type=int, choices=(512, 4096), required=True)
        p.add_argument("--dossier", type=Path, required=True)
        p.add_argument("--oracle", type=Path, required=True)
        preuve_invitee(p.parse_args())
    else:
        unittest.main()
