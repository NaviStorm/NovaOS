from dataclasses import replace
import unittest
from ats import Bail, Retrait


class Caches(unittest.TestCase):
    def setUp(self):
        self.bail = Bail(19, 3, 1, 2, 3, frozenset({(56, None), (57, 7)}))
        self.r = Retrait(self.bail, 8)
        self.r.fermer()

    def commencer(self):
        return self.r.retirer(
            fin_transactions=True, portee_exclusive=True, contexte_et_iotlb=True
        )

    def rendre(self):
        return self.r.rendre(self.bail, 8, wait_ordonne=True)

    def repondre(self, cache, bail=None, seq=8, fin=True):
        return self.r.reponse_atc(
            bail or self.bail, seq, cache, achevement_endpoint=fin
        )

    def test_aucun_reemploi_avant_les_deux_caches(self):
        self.assertTrue(self.commencer())
        self.assertFalse(self.rendre())
        self.assertTrue(self.repondre((57, 7)))
        self.assertFalse(self.rendre())
        self.assertFalse(self.repondre((57, 7)))
        self.assertTrue(self.repondre((56, None)))
        self.assertFalse(self.r.rendre(self.bail, 8, wait_ordonne=False))
        self.assertTrue(self.rendre())
        self.assertFalse(self.rendre())

    def test_portee_du_recu(self):
        self.commencer()
        for champ in (
            "epoque",
            "titulaire",
            "incarnation",
            "generation",
            "domaine",
            "caches",
        ):
            autre = replace(
                self.bail,
                **{
                    champ: frozenset({(56, None)})
                    if champ == "caches"
                    else getattr(self.bail, champ) + 1
                },
            )
            self.assertFalse(self.repondre((56, None), autre))
        self.assertFalse(self.repondre((56, None), seq=7))
        self.assertFalse(self.repondre((56, 7)))
        self.assertFalse(self.repondre((56, None), fin=False))
        self.assertFalse(self.rendre())

    def test_panne_commune_et_reponse_tardive(self):
        self.commencer()
        self.repondre((56, None))
        self.r.panne()
        self.assertFalse(self.repondre((57, 7)))
        self.assertFalse(self.rendre())
        self.assertEqual(self.r.phase, "quarantaine")

    def test_drainage_incomplet(self):
        self.assertFalse(
            self.r.retirer(
                fin_transactions=False, portee_exclusive=True, contexte_et_iotlb=True
            )
        )
        self.assertFalse(self.repondre((56, None)))
        self.assertFalse(self.rendre())

    def test_invalidation_non_confirmee(self):
        for portee, iotlb in ((False, True), (True, False)):
            r = Retrait(self.bail, 9)
            r.fermer()
            self.assertFalse(
                r.retirer(
                    fin_transactions=True,
                    portee_exclusive=portee,
                    contexte_et_iotlb=iotlb,
                )
            )
            self.assertEqual(r.phase, "quarantaine")

    def test_cache_inconnu(self):
        for caches in (
            frozenset(),
            frozenset({(-1, None)}),
            frozenset({(56, 1 << 20)}),
        ):
            with self.assertRaises(ValueError):
                Retrait(replace(self.bail, caches=caches), 8)
