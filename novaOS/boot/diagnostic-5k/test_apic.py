import unittest
from verdict_apic import INT, REG, SOURCE, juger


class ExceptionMinuterie(unittest.TestCase):
    def setUp(self):
        self.start = SOURCE + b"\n" + INT + b"\n"
        self.store = b"memory_region_ops_write cpu 0 mr 0xcafe addr 0xfee000b0 value 0x0 size 4 name 'apic-msi'\n"
        self.row = dict(
            entree=len(self.start),
            avant_ecriture=len(self.start),
            apres_hlt=True,
            vcpu=0,
            vecteur=32,
            gestionnaire=0x4B9,
            instruction=0x97F,
            retour=0x4DF,
            adresse=0xFEE000B0,
            valeur=0,
            lapic="dumping local APIC state for CPU 0\nISR  32\nLVTT  0x00020020 periodic Fixed  (vec 32)",
        )
        self.buf = self.start + self.store + REG + b"\n"

    def verdict(self, buf=None, row=None):
        buf = self.buf if buf is None else buf
        return juger(
            buf, buf, dict(debut=0, fin=len(buf), timer=0, lignes=[row or self.row])
        )

    def test_exception_bornee(self):
        self.assertEqual(
            self.verdict(),
            dict(eoi_attribues=1, ecritures_refusees=[], intervalle_admis=True),
        )

    def test_ecriture_supplementaire(self):
        for extra in (
            self.store,
            b"pci_cfg_write autre\n",
            b"memory_region_ops_write apm-io\n",
            b"memory_region_subpage_write autre\n",
            b"vtd_reg_write autre\n",
        ):
            with self.subTest(extra=extra):
                self.assertFalse(self.verdict(self.buf + extra)["intervalle_admis"])

    def test_attribution(self):
        for k in (
            "apres_hlt",
            "vcpu",
            "vecteur",
            "gestionnaire",
            "instruction",
            "retour",
            "adresse",
            "valeur",
            "lapic",
        ):
            with self.subTest(k=k), self.assertRaises(ValueError):
                self.verdict(
                    row=self.row | {k: "" if k == "lapic" else not self.row[k]}
                )

    def test_lacunes_et_ecritures_divergentes(self):
        for old, new in (
            (b"size 4", b"size 8"),
            (b"value 0x0", b"value 0x1"),
            (b"cpu 0", b"cpu 1"),
            (REG, b""),
            (INT, INT.replace(b"20", b"21")),
            (SOURCE, SOURCE.replace(b"vector 0", b"vector 3")),
        ):
            with self.subTest(old=old), self.assertRaises(ValueError):
                self.verdict(self.buf.replace(old, new))
        with self.assertRaises(ValueError):
            juger(
                self.buf,
                self.buf[:-1],
                dict(debut=0, fin=len(self.buf), timer=0, lignes=[self.row]),
            )

    def test_irq_peripherique(self):
        for irq in (b"pci_nvme_irq_msix", b"virtio_notify", b"apic_deliver_irq"):
            with self.subTest(irq=irq), self.assertRaises(ValueError):
                self.verdict(self.buf + irq + b"\n")
