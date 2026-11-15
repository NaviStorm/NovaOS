"""Contre-exemples pour les oracles indépendants des coupures NVMe."""
from pathlib import Path
import tempfile
import unittest

from nvme_phase2_oracle import BLOCK, GUARD, RAW_BYTES, chronology, io_trace, unconfirmed_write, view, virgin


class Proofs(unittest.TestCase):
    def test_guard_unchanged_outside_the_grant(self):
        with tempfile.TemporaryDirectory() as root:
            image = Path(root)/"disque"
            virgin(image)
            self.assertEqual(len(view(image)),RAW_BYTES-BLOCK)
            with image.open("r+b") as f:
                f.seek(-1,2)
                f.write(bytes([GUARD[-1]^1]))
            with self.assertRaises(ValueError):
                view(image)

    def test_partial_and_confirmed_publication(self):
        writes = [
            {"cid": 7, "op": "WRITE", "lba": 32, "blocs": 8, "cqe": 0},
            {"cid": 8, "op": "FLUSH", "cqe": 0},
            {"cid": 9, "op": "WRITE", "lba": 8, "blocs": 8, "cqe": 0},
            {"cid": 10, "op": "FLUSH", "cqe": 0}]
        self.assertEqual(chronology(writes,{"slot":0},512)["flush_cid"], [8,10])
        with self.assertRaises(ValueError):
            chronology(writes[:3],{"slot":0},512)
        with self.assertRaises(ValueError):
            chronology(writes,{"slot":1},512)
        with self.assertRaises(ValueError):
            chronology(writes,{"slot":0},512,"during-f2")
        del writes[-1]["cqe"]
        self.assertEqual(chronology(writes,{"slot":0},512,"during-f2")["ancre_cid"], [9])

    def test_trace_rejects_a_write_to_the_guard(self):
        with tempfile.TemporaryDirectory() as root:
            trace = Path(root)/"trace"
            trace.write_text("pci_nvme_io_cmd cid 7 nsid 0x1 sqid 1 opc 0x1 opname 'NVME_NVM_CMD_WRITE'\n"
                             "pci_nvme_write cid 7 opname 'write' nsid 1 nlb 1 count 4096 lba 0x3ff\n")
            with self.assertRaises(ValueError):
                io_trace(trace,4096)

    def test_fault_only_changes_submitted_sectors(self):
        with tempfile.TemporaryDirectory() as root:
            seed, image = Path(root)/"initial", Path(root)/"apres"
            virgin(seed)
            virgin(image)
            write = [{"op":"WRITE", "lba":32, "blocs":1}]
            with image.open("r+b") as f:
                f.seek(32*512)
                f.write(b"abc")
            self.assertEqual(unconfirmed_write(seed,image,write,512),
                             {"octets_changes":3,"blocs_p9_changes":[4]})
            with image.open("r+b") as f:
                f.seek(33*512)
                f.write(b"x")
            with self.assertRaises(ValueError):
                unconfirmed_write(seed,image,write,512)


if __name__ == "__main__":
    unittest.main()
