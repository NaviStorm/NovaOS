from dataclasses import replace
import unittest

from restitution import Autorite, Bail


class Restitution(unittest.TestCase):
    def setUp(self):
        self.bail = Bail(bytes(range(16)), "pilote", 8, 3, 2, 4, (4096, 8192))
        self.a = Autorite(self.bail)

    def pending(self):
        self.a.soumettre(7)
        self.a.fermer()
        self.a.achever(0, 7)
        self.a.drainer()
        self.a.preparer_invalidation(19, 48)

    def test_bail(self):
        for k, v in (
            ("rid", -1),
            ("domaine", 0),
            ("titulaire", ""),
            ("epoque", bytes(16)),
            ("pages", (4096, 4096)),
            ("pages", [4096]),
            ("pages", (0,)),
            ("pages", (8193,)),
            ("generation", 0),
            ("incarnation", 0),
        ):
            with self.subTest(k=k, v=v), self.assertRaises(ValueError):
                Autorite(replace(self.bail, **{k: v}))

    def test_disparition_en_vol(self):
        self.a.soumettre(7)
        self.a.fermer()
        for op in (
            lambda: self.a.soumettre(8),
            self.a.drainer,
            lambda: self.a.achever(1, 7),
            lambda: self.a.achever(0, 6),
            lambda: self.a.restituer(None, lambda _: True),
        ):
            with self.assertRaises(ValueError):
                op()
        self.assertEqual(self.a.etat, "drainage")
        self.assertEqual(self.a.attendu, 7)

    def test_attente_retardee_absente_contradictoire(self):
        self.pending()
        for args in (
            (0, 0, 48, 0, True),
            (4, 48, 48, 0, True),
            (19, 0, 48, 0, True),
            (19, 64, 64, 0, True),
        ):
            with self.subTest(args=args), self.assertRaises(ValueError):
                self.a.confirmer(*args)
        with self.assertRaises(ValueError):
            self.a.preparer_invalidation(4, 64)
        recu = self.a.confirmer(19, 48, 48, 0, True)
        self.assertEqual(recu.cookie, 19)
        with self.assertRaises(ValueError):
            self.a.confirmer(19, 48, 48, 0, True)

    def test_faute_commune_ou_tables_changees(self):
        for faute, tables in ((16, True), (0, False)):
            self.a = Autorite(self.bail)
            self.pending()
            with self.assertRaises(ValueError):
                self.a.confirmer(19, 48, 48, faute, tables)
            self.assertEqual(self.a.etat, "quarantaine")
            with self.assertRaises(ValueError):
                self.a.confirmer(19, 48, 48, 0, True)

    def test_recu_unique_et_lie_au_bail(self):
        self.pending()
        recu = self.a.confirmer(19, 48, 48, 0, True)
        for k, v in (
            ("rid", 16),
            ("domaine", 1),
            ("titulaire", "disque"),
            ("epoque", bytes(reversed(range(16)))),
            ("pages", (12288,)),
            ("generation", 3),
            ("incarnation", 3),
        ):
            with self.subTest(k=k), self.assertRaises(ValueError):
                self.a.restituer(
                    replace(recu, bail=replace(self.bail, **{k: v})),
                    lambda p: self.fail(),
                )
        for faux in (
            None,
            replace(recu),
            replace(recu, sequence=5),
            replace(recu, cookie=20),
        ):
            with self.assertRaises(ValueError):
                self.a.restituer(faux, lambda p: self.fail())
        pages = []

        def nettoyer(p):
            pages.append(p)
            return True

        self.assertEqual(self.a.restituer(recu, nettoyer), self.bail.pages)
        with self.assertRaises(ValueError):
            self.a.restituer(recu, nettoyer)
        self.assertEqual(tuple(pages), self.bail.pages)

    def test_effacement_partiel(self):
        self.pending()
        recu = self.a.confirmer(19, 48, 48, 0, True)
        with self.assertRaises(ValueError):
            self.a.restituer(recu, lambda p: p == 4096)
        self.assertEqual(self.a.etat, "quarantaine")
        with self.assertRaises(ValueError):
            self.a.restituer(recu, lambda _: True)

    def test_recu_autre_autorite(self):
        self.pending()
        recu = self.a.confirmer(19, 48, 48, 0, True)
        autre = Autorite(self.bail)
        autre.fermer()
        autre.drainer()
        autre.preparer_invalidation(19, 48)
        autre.confirmer(19, 48, 48, 0, True)
        with self.assertRaises(ValueError):
            autre.restituer(recu, lambda p: self.fail())

    def test_invalidation_non_commandee(self):
        self.a.fermer()
        self.a.drainer()
        for cookie, q in ((0, 48), (1, 0), (1, 3), (1, 4096), (1 << 32, 48)):
            with self.assertRaises(ValueError):
                self.a.preparer_invalidation(cookie, q)
        with self.assertRaises(ValueError):
            self.a.confirmer(0, 0, 0, 0, True)
