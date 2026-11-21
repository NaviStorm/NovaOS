"""Conversion réellement compilée ; complément des essais de délais dans QEMU."""
import os,re,subprocess,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
class Deadline(unittest.TestCase):
    def test_native_units_and_limits(self):
        scheduler=(ROOT/'novaOS/kernel/src/sched/mod.rs').read_text()
        self.assertRegex(scheduler,r'pub const HZ: u32 = 250;')
        source=ROOT/'novaOS/user/novaauth/service/src/bin/common/ipc_deadline.rs'
        with tempfile.TemporaryDirectory(dir=os.environ['NOVAOS_TMPDIR']) as d:
            binary=Path(d)/'deadline'
            subprocess.run(['rustup','run','stable','rustc','--edition=2024','--test',str(source),'-o',str(binary)],check=True)
            subprocess.run([str(binary)],check=True)
if __name__=='__main__':unittest.main()
