import unittest
from dataclasses import replace
from struct import pack
from contrat import Bail, Constat, verifier


def remappeur(base, sid, depart):
    def page(*words):
        return pack("<512Q", *words, *([0] * (512 - len(words))))

    root, ctx, l4, l3, l2, l1, data = (depart + i * 4096 for i in range(7))
    pages = {
        root: page(ctx | 1),
        ctx: page(*([0] * (2 * sid)), l4 | 1, 0x102),
        l4: page(l3 | 3),
        l3: page(l2 | 3),
        l2: page(l1 | 3),
        l1: page(data | 2),
    }
    regs = dict(ver=16, cap=(47 << 16) | (1 << 10) | 6, rtaddr=root, gsts=1 << 31)
    return (
        Constat(base, regs, pages, frozenset(pages), frozenset([sid])),
        Bail(base, sid, 1, ((0, data, 2),)),
    )


class Contrat(unittest.TestCase):
    def setUp(self):
        self.u, self.b = remappeur(0x100000, 8, 4096)
        self.v, self.c = remappeur(0x200000, 16, 0x10000)
        self.args = dict(
            epoque=b"a" * 16,
            epoque_courante=b"a" * 16,
            zones=[(0x80000, 0x90000)],
            unites=[self.u, self.v],
            baux=[self.b, self.c],
            demandeurs={8, 16},
            rmrr=[],
            attribution_complete=True,
            en_vol_inconnu=False,
        )

    def refus(self, **kw):
        with self.assertRaises(ValueError):
            verifier(**(self.args | kw))

    def test_deux_unites_disjointes(self):
        self.assertEqual(verifier(**self.args), 12 * 4096)

    def test_identites_et_alias(self):
        self.refus(unites=[self.u, self.u])
        self.refus(unites=[self.u, replace(self.v, demandeurs={8, 16})])
        self.refus(demandeurs={8, 16, 17})
        self.refus(attribution_complete=False)

    def test_receveur_et_transactions(self):
        self.refus(epoque_courante=b"b" * 16)
        self.refus(epoque=b"")
        self.refus(en_vol_inconnu=True)

    def test_permission_et_domaine(self):
        self.refus(baux=[replace(self.b, feuilles=((0, 7 * 4096, 3),)), self.c])
        self.refus(baux=[replace(self.b, domaine=2), self.c])
        self.refus(baux=[self.b])

    def test_conflits_physiques_et_rmrr(self):
        self.refus(zones=[(4096, 8192)])
        self.refus(zones=[(7 * 4096, 8 * 4096)])
        self.refus(zones=[(1, 4096)])
        self.refus(rmrr=[(0x80000, 0x81000)])
        self.refus(rmrr=[(0x60000, 0x61000)])

    def test_provenance_tables(self):
        self.refus(unites=[replace(self.u, cadres_tables=frozenset()), self.v])
        self.refus(
            unites=[replace(self.u, registres=self.u.registres | dict(gsts=0)), self.v]
        )


if __name__ == "__main__":
    unittest.main()
