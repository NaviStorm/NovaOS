"""Tests de refus sur structures synthétiques et événements ordonnés du témoin."""

import copy
import struct
import unittest
from observation import activite, etendue
from origine import chemin
from temoin_usb import chemin_usb, garde, lecture_hotplug, tampons


class CheminTests(unittest.TestCase):
    def setUp(self):
        self.part = ["partition", 1, 2048, 67584, "00" * 16, 2, 2]
        self.origin = {
            "peripherique": [
                ["acpi", 0, 0],
                ["pci", 5, 1],
                ["usb", 6, 0],
                ["usb", 3, 0],
                self.part,
            ]
        }
        self.bus = "/machine/peripheral/a/a.0"
        self.bds = (
            "starting Boot0001 from PciRoot(0x0)/Pci(0x5,0x1)/USB(0x5,0x0)/USB(0x2,0x0)"
        )
        self.slot = dict(
            slot=9,
            etat=3,
            adresse_usb=7,
            port_racine=6,
            route=3,
            endpoints=[dict(type=t, dci=d) for t, d in ((4, 1), (2, 2), (6, 5))],
        )
        self.ctrl = dict(
            qdev="a", ports=[6], slots=[self.slot], protocoles=[[2, 5, 4], [3, 1, 4]]
        )
        self.inv = dict(
            controleurs={"0000:00:05.1": self.ctrl},
            bus_usb=["Device 1.7, Port 2.3, Speed 480 Mb/s, Product MSD, ID: cle1"],
        )

    def attribuer(self):
        return chemin_usb(self.inv, self.origin, self.bds, self.part, self.bus)

    def test_route_usb2_derriere_hub(self):
        a = self.attribuer()
        self.assertEqual(
            (a["rid"], a["chemin_hub"], a["slot"], a["adresse"]),
            ("0000:00:05.1", "2.3", 9, 7),
        )

    def test_bds_etranger(self):
        self.bds += "/USB(0x0,0x0)"
        with self.assertRaises(ValueError):
            self.attribuer()

    def test_slot_duplique(self):
        self.ctrl["slots"].append(copy.deepcopy(self.slot))
        with self.assertRaises(ValueError):
            self.attribuer()

    def test_interface_ou_lun_non_couvert(self):
        self.origin["peripherique"].insert(-1, ["lun", 1])
        with self.assertRaises(ValueError):
            self.attribuer()

    def test_partition_substituee(self):
        self.origin["peripherique"][-1] = self.part[:]
        self.origin["peripherique"][-1][3] += 1
        with self.assertRaises(ValueError):
            self.attribuer()

    def test_bus_contradictoire(self):
        self.bus = "/machine/peripheral/b/b.0"
        with self.assertRaises(ValueError):
            self.attribuer()

    def test_port_deconnecte(self):
        self.ctrl["ports"].clear()
        with self.assertRaises(ValueError):
            self.attribuer()

    def test_protocoles_superposes(self):
        self.ctrl["protocoles"].append([3, 6, 1])
        with self.assertRaises(ValueError):
            self.attribuer()


class RessourcesTests(unittest.TestCase):
    def test_ram_avec_trou(self):
        with self.assertRaises(ValueError):
            etendue(4096, 4096, 16, [(4096, 6144), (7168, 8192)])

    def test_device_path_inconnu(self):
        raw = struct.pack("<BBH", 127, 1, 4)
        with self.assertRaises(ValueError):
            chemin(lambda a, n: raw[a : a + n], 0)

    def test_completions_et_reemploi_de_requete(self):
        start = "usb_xhci_xfer_start 0x123: slotid 7, epid 3, streamid 0"
        done = "usb_xhci_xfer_success 0x123: len 512"
        self.assertEqual(activite([start, done, start, done])["anomalies"], 0)
        self.assertEqual(activite([start, start, done])["anomalies"], 1)
        self.assertEqual(activite([done])["anomalies"], 1)

    def test_donnees_immediates_ne_sont_pas_des_adresses(self):
        trb = "usb_xhci_fetch_trb addr 0x00001000, TR_NORMAL, p 0x{:08x}, s 0x00000010, c 0x{:08x}\n"
        raw = (trb.format(8192, 1) + trb.format(0xDEADBEEF, 65)).encode()
        t = tampons(raw, [(4096, 12288)], 8192, 4096)
        self.assertEqual(t["etendues"], [(8192, 16)])
        self.assertEqual(t["recouvrements"], 1)

    def test_tampon_hors_ram(self):
        raw = b"usb_xhci_fetch_trb addr 0x1000, TR_NORMAL, p 0xfff0, s 0x20, c 0x1\n"
        with self.assertRaises(ValueError):
            tampons(raw, [(4096, 65536)], 8192, 4096)

    def test_controle_lecture_du_modele(self):
        inner = b"blk_root_attach child 0x3 blk 0x2 bs 0x1\nblk_co_preadv blk 0x2 bs 0x1 offset 0 bytes 512 flags 0x0\n"
        self.assertEqual(lecture_hotplug(inner, 0, len(inner), 0, len(inner)), 512)
        for raw in (
            inner.replace(b"bs 0x1 offset", b"bs 0x5 offset"),
            inner + b"blk_co_pwritev \n",
            inner.replace(b"512", b"4096"),
        ):
            with self.assertRaises(ValueError):
                lecture_hotplug(raw, 0, len(raw), 0, len(raw))

    def test_motifs_de_quarantaine(self):
        for sources, change, pending, reason in (
            ([], None, None, "support absent"),
            (["a", "b"], None, None, "plusieurs médias identiques"),
            (["a"], {}, True, "requête héritée en vol"),
            (["a"], {"generation_apres": 2}, None, "incarnation remplacée"),
        ):
            r = garde("ab" * 16, {}, sources, change, pending)
            self.assertEqual(r["motif"], reason)
            self.assertFalse(r["droit_ecrire"] or r["droit_restituer"])

    def test_epoque_manquante(self):
        with self.assertRaises(ValueError):
            garde("", {}, ["a"], None, None)


if __name__ == "__main__":
    unittest.main()
