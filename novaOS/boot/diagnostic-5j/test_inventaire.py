import unittest
import struct

from inventaire import couverture


class Inventaire(unittest.TestCase):
    def setUp(self):
        self.base = 0xABC000
        scope = bytes([1, 8, 0, 0, 0, 0, 2, 0])
        ioapic = bytes([3, 8, 0, 0, 0, 255, 0, 0])
        self.head = bytes(48)
        self.drhd = struct.pack("<HHBBHQ", 0, 32, 0, 0, 0, self.base) + ioapic + scope

    def test_attribution(self):
        self.assertEqual(couverture(self.head + self.drhd, self.base, {16}), {16})

    def test_unite_et_rmrr(self):
        for extra in (self.drhd, struct.pack("<HH", 1, 24) + bytes(20)):
            with self.assertRaises(ValueError):
                couverture(self.head + self.drhd + extra, self.base, {16})

    def test_alias_et_inconnu(self):
        for index, valeur in ((30, 3), (24, 2), (17, 7), (21, 0), (28, 1)):
            blob = bytearray(self.drhd)
            blob[index] = valeur
            with self.assertRaises(ValueError):
                couverture(
                    self.head + blob, self.base, {16, 0} if index == 21 else {16}
                )

    def test_troncature(self):
        for fin in range(4, 32):
            with self.assertRaises(ValueError):
                couverture(self.head + self.drhd[:fin], self.base, {16})


if __name__ == "__main__":
    unittest.main()
