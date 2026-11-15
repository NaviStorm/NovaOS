"""Contrôles de cohérence, de compatibilité historique et des consommateurs C/C++."""
import contextlib
import copy
import io
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import tomllib
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import generate_abi as abi

BASE = "a6df93fb8687c68893680707f8763471d9f8a2b3"
LIVRAISON = "3514e7804f4c5ea7a0e79d4bfa57632c3938ac9e"


def historical(commit, file):
    return subprocess.check_output(["git", "show", f"{commit}:novaOS/{file}"], cwd=ROOT)


class Registre(unittest.TestCase):
    def setUp(self):
        self.registry = abi.read_toml(abi.ABI / "syscalls.toml")
        self.template = (ROOT / self.registry["layout_source"]).read_text()
        self.calls = abi.validate(self.registry, self.template)

    def test_regeneration(self):
        outputs = abi.outputs()
        self.assertEqual(outputs, abi.outputs())
        for path, content in outputs.items():
            self.assertEqual(content.encode(), path.read_bytes(), path)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "produit.rs"
            path.write_text("ancien")
            with patch.object(abi, "outputs", return_value={path: "nouveau"}), \
                 patch.object(sys, "argv", ["generate_abi.py", "--check"]), \
                 contextlib.redirect_stderr(io.StringIO()) as diagnostic, \
                 self.assertRaises(SystemExit) as caught:
                abi.main()
            self.assertEqual(caught.exception.code, 1)
            self.assertIn("régénération requise", diagnostic.getvalue())
            self.assertEqual(path.read_text(), "ancien")

    def test_invalides(self):
        with self.assertRaises(tomllib.TOMLDecodeError):
            tomllib.loads("[call.X]\nnumber=1\n[call.X]\nnumber=2\n")
        for number, diagnostic in ((self.calls["IPC_SEND"]["number"], "dupliqué"),
                                   (self.registry["reserved"][0][0], "réservé"), (3, "invalide")):
            changed = copy.deepcopy(self.registry)
            changed["call"]["IPC_CREATE"]["number"] = number
            with self.assertRaisesRegex(ValueError, diagnostic):
                abi.validate(changed, self.template)
        with self.assertRaisesRegex(ValueError, "fonction absente"):
            abi.link("user/gate.rs#fonction_absente")

    def test_signatures_historiques(self):
        for module in abi.MODULES:
            old = historical(BASE, f"abi/{module}.rs").decode()
            numbers = {name: int(value, 16) for name, value in re.findall(
                r"pub const (\w+): u64 = (0x[\da-fA-F_]+);", old)
                if int(value, 16) >> 16 == 1}
            for name, value in numbers.items():
                self.assertEqual((module, value), (self.calls[name]["module"], self.calls[name]["number"]))
        # Autoriser des extensions tout en contrôlant les déclarations de l’archive v0.20.
        previous = historical(LIVRAISON, "abi/native.rs").decode()
        current = (abi.ABI / "native.rs").read_text()
        for pattern in (r"pub const (\w+): (\w+) = ([^;]+);",
                        r"#\[repr\(C\)\]\s*(?:#\[derive\([^\n]*\)\]\s*)?pub struct (\w+) \{([^}]+)\}",
                        r"(offset_of!\(\w+, \w+\) == \d+)"):
            old = set(re.findall(pattern, previous))
            self.assertTrue(old)
            self.assertLessEqual(old, set(re.findall(pattern, current)))

    def test_portes_et_dispatch(self):
        rust_gate = (ROOT / "user/gate.rs").read_text().lower()
        for arch, gate in self.registry["calling"].items():
            self.assertIn(gate["instruction"].lower(), rust_gate)
            for reg in [gate["selector"], *gate["arguments"], *gate["reply"]]:
                self.assertIn('"' + reg.lower() + '"', rust_gate)
        for file in ("kernel/src/main.rs", "libs/nova-system/src/lib.rs"):
            self.assertIn("abi/native.rs", (ROOT / file).read_text())
        dispatch = (ROOT / "kernel/src/sched/process/syscall.rs").read_text()
        self.assertIn("THREAD_CREATE..=THREAD_NOTIFY", dispatch)
        self.assertIn("IPC_CREATE..=IPC_BOOTSTRAP", dispatch)
        for name, spec in self.calls.items():
            path = ROOT / spec["handler"].split("#")[0]
            self.assertRegex(dispatch + path.read_text(), rf"\b{name}\b")

    def test_selecteurs_litteraux(self):
        files = subprocess.check_output(["git", "ls-files", "-z", "--", "*.rs", "*.S", "*.s"],
                                        cwd=ROOT.parent).decode().split("\0")
        known = {s["number"] for s in self.calls.values()}
        generated = {p.resolve() for p in abi.outputs()}
        probes = {"novaOS/user/" + name for name in
                  ("x86_64.S", "aarch64.S", "fp_x86_64.S", "fp_aarch64.S")}
        probes_bad_id = {
            "novaOS/user/calls.rs": {0x1ffff, 0x20001},
            "novaOS/kernel/src/sched/process/ipc/probes/contract.rs": {0x1002a, 0x20020},
            "novaOS/tests/reservations_endurance_invite.rs": {0x1003a, 0x10032},
        }
        for file in filter(None, files):
            path = ROOT.parent / file
            if path.resolve() in generated or file in probes:
                continue
            text = path.read_text()
            calls = re.findall(r"\bcall\(\s*(0x[\da-fA-F_]+|\d+)\s*,", text)
            self.assertFalse({int(v, 0) for v in calls} - probes_bad_id.get(file, set()), file)
            literals = re.findall(r"\b(?:call\s*\(|const\s+\w+\s*:\s*u64\s*=|"
                                  r"mov\s+(?:e?ax|rax|x8)\s*,\s*#?)\s*(0x[\da-fA-F_]+|\d+)\b", text)
            self.assertFalse({int(v, 0) for v in literals} & known, file)

    def test_entetes_c_cpp(self):
        for target in ("x86_64-none-elf", "aarch64-none-elf"):
            for compiler, language, standard in (("clang", "c", "c11"), ("clang++", "c++", "c++11")):
                assertion = "_Static_assert" if language == "c" else "static_assert"
                test = '#include "nova_abi.h"\n' + "\n".join(
                    f'{assertion}(NOVA_SYS_{name} == {s["number"]}, "{name}");'
                    for name, s in self.calls.items())
                subprocess.run([compiler, "-target", target, "-ffreestanding", "-fsyntax-only",
                                "-pedantic-errors", "-Wall", "-Werror", "-std=" + standard,
                                "-I", abi.ABI / "generated", "-x", language, "-"],
                               input=test + "\n", text=True, check=True)


if __name__ == "__main__":
    unittest.main()
