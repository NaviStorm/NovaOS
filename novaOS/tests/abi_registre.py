"""Contrôles de cohérence, de compatibilité historique et des consommateurs C/C++."""
import contextlib
import copy
import io
import json
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
        changed = copy.deepcopy(self.registry)
        changed["call"]["TMP_NEXT"]["rust_name"] = "CALL"
        with self.assertRaisesRegex(ValueError, "Rust dupliquée"):
            abi.validate(changed, self.template)
        with self.assertRaisesRegex(ValueError, "fonction absente"):
            abi.link("user/gate.rs#fonction_absente")

    def test_signatures_historiques(self):
        qualified = tomllib.loads(historical("df88f1cfeaf0940da876ea543584e59215abacd4", "abi/syscalls.toml").decode())
        for name, spec in qualified["call"].items():
            self.assertEqual(self.calls[name], spec, name)
        symbols = {(spec["module"], spec.get("rust_name", key)): spec["number"]
                   for key, spec in self.calls.items()}
        for module in ("native", "terminal", "block", "instances", "tmp"):
            commit = "35f7d9619f116f42491f001af4b1cf5bdc84b395" if module == "tmp" else BASE
            old = historical(commit, f"abi/{module}.rs").decode()
            numbers = {name: int(value, 16) for name, value in re.findall(
                r"pub const (\w+): u64 = (0x[\da-fA-F_]+);", old)
                if int(value, 16) >> 16 == 1}
            for name, value in numbers.items():
                self.assertEqual(symbols[module, name], value)
        # Autoriser des extensions tout en contrôlant les déclarations de l’archive v0.20.
        previous = historical(LIVRAISON, "abi/native.rs").decode()
        current = (abi.ABI / "native.rs").read_text()
        for pattern in (r"pub const (\w+): (\w+) = ([^;]+);",
                        r"#\[repr\(C\)\]\s*(?:#\[derive\([^\n]*\)\]\s*)?pub struct (\w+) \{([^}]+)\}",
                        r"(offset_of!\(\w+, \w+\) == \d+)"):
            old = set(re.findall(pattern, previous))
            self.assertTrue(old)
            self.assertLessEqual(old, set(re.findall(pattern, current)))

    def test_extension_services(self):
        previous = tomllib.loads(historical(
            "bcdfe4ccfd98c9e4a58f5cead3ce666e917ce33c", "abi/syscalls.toml").decode())
        self.assertEqual(len(previous["call"]), 46)
        self.assertEqual({k: self.calls[k] for k in previous["call"]}, previous["call"])
        self.assertEqual(set(self.calls)-set(previous["call"]), {"SERVICE_PROCESS", "CACHE_CONTROL"})
        phase3 = tomllib.loads(historical(
            "aa2c44a81e3ce444a75db40efa91f91f68b093a4", "abi/syscalls.toml").decode())["call"]
        self.assertEqual(len(phase3), 47)
        self.assertEqual({k: self.calls[k] for k in phase3}, phase3)
        self.assertEqual(self.calls["CACHE_CONTROL"]["number"], 0x1007c)
        self.assertEqual(self.calls["SERVICE_PROCESS"]["number"], 0x1007b)
        self.assertEqual(self.calls["SERVICE_PROCESS"]["module"], "instances")

    def test_portes_et_dispatch(self):
        rust_gate = (ROOT / "user/gate.rs").read_text().lower()
        for arch, gate in self.registry["calling"].items():
            self.assertIn(gate["instruction"].lower(), rust_gate)
            for reg in [gate["selector"], *gate["arguments"], *gate["reply"]]:
                self.assertIn('"' + reg.lower() + '"', rust_gate)
        for file in ("kernel/src/main.rs", "libs/nova-system/src/lib.rs"):
            self.assertIn("abi/native.rs", (ROOT / file).read_text())
        dispatch = (ROOT / "kernel/src/sched/process/syscall.rs").read_text()
        prefixes = {"": "native", "iw": "instances", "tmp::wire": "tmp", "cache::wire": "cache",
                    "crate::terminal": "terminal", "crate::hardware::block": "block",
                    "crate::hardware::block::wire": "block"}
        numbers = {(spec["module"], spec.get("rust_name", key)): spec["number"]
                   for key, spec in self.calls.items()}

        def resolve(token):
            prefix, _, name = token.rpartition("::")
            return numbers[prefixes[prefix], name]

        # Lire les motifs des branches externes du match. Les refus préalables
        # des threads secondaires ne comptent pas comme un traitement du syscall.
        body = dispatch.split("    let result = match id {", 1)[1].split("\n    };", 1)[0]
        body = re.sub(r"\n        \|", " |", body)
        seen = set()
        for line in body.splitlines():
            if not re.match(r"^        [\w]", line):
                continue
            pattern, sep, _ = line.strip().partition(" =>")
            self.assertTrue(sep, line)
            cases, _, guard = pattern.partition(" if ")
            if cases == "_" or guard == "secondary":
                continue
            for case in cases.split(" | "):
                first, interval, last = case.partition("..=")
                if interval:
                    seen.update(range(resolve(first), resolve(last) + 1))
                else:
                    seen.add(resolve(first))
        self.assertEqual(seen, set(numbers.values()))

        for key, spec in self.calls.items():
            name = spec.get("rust_name", key)
            path = ROOT / spec["handler"].split("#")[0]
            self.assertRegex(dispatch + path.read_text(), rf"\b{name}\b")

    def test_statut_tmp_call(self):
        call = self.calls["TMP_CALL"]
        self.assertIn("1…255", call["result"])
        self.assertIn("ce statut à l’identique, valeur 0", call["result"])
        self.assertIn("erreurs propres au courtier", call["result"])
        self.assertIn("REFUSED", call["errors"])
        for status in ("CLOSED", "files::UNCERTAIN", "BAD_ADDRESS", "DENIED"):
            self.assertIn(status, call["result"])
            self.assertIn(status, call["errors"])
        self.assertIn("0…255", self.calls["TMP_COMPLETE"]["arguments"])
        broker = (ROOT / "kernel/src/sched/process/tmp.rs").read_text()
        complete = broker.split("fn complete(", 1)[1].split("pub(super) fn call(", 1)[0]
        self.assertIn("if status > 255 ||", complete)
        self.assertIn("r.done = Some(status);", complete)
        io = broker.split("fn io(", 1)[1].split("fn next(", 1)[0]
        self.assertRegex(io, r"if status != OK \{\s*return Err\(status\);")
        self.assertIn("return Ok(wire::FRAME as u64)", io)
        dispatch = (ROOT / "kernel/src/sched/process/syscall.rs").read_text()
        self.assertIn("Err(status) => Reply { status, value: 0 }", dispatch)

    def test_exports_tmp(self):
        header = (abi.ABI / "generated/nova_abi.h").read_text()
        old = historical("df88f1cfeaf0940da876ea543584e59215abacd4", "abi/generated/nova_abi.h").decode()
        self.assertLessEqual({line for line in old.splitlines() if line.startswith("#define NOVA_SYS_")},
                             set(header.splitlines()))
        added = {"CAP": "0x10076", "CALL": "0x10077", "WORKER": "0x10078", "NEXT": "0x10079", "COMPLETE": "0x1007A"}
        for name, literal in added.items():
            self.assertIn(f"#define NOVA_SYS_TMP_{name} {literal}", header.splitlines())
        self.assertIn('include!("generated/tmp.rs");', (abi.ABI / "tmp.rs").read_text())
        for path in ("kernel/src/sched/process/tmp.rs", "user/tmp_sample.rs", "user/novafs/files/temporary.rs"):
            self.assertIn("abi/tmp.rs", (ROOT / path).read_text())

        source = ["#![no_std]", "#![allow(dead_code)]"]
        for module in abi.MODULES:
            path = json.dumps(str(abi.ABI / f"{module}.rs"), ensure_ascii=False)
            source.append(f"#[path = {path}] mod {module};")
        for key, spec in self.calls.items():
            symbol = spec["module"] + "::" + spec.get("rust_name", key)
            source.append(f"const _: () = assert!({symbol} == {spec['number']}u64);")
        with tempfile.TemporaryDirectory() as tmp:
            for target in ("x86_64-unknown-none", "aarch64-unknown-none-softfloat"):
                subprocess.run(["rustup", "run", "stable", "rustc", "--edition=2024", "--crate-type=lib",
                                "--emit=metadata", "--target", target, "-o", Path(tmp) / f"{target}.rmeta", "-"],
                               input="\n".join(source), text=True, check=True)

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
                    f'{assertion}((uint64_t)NOVA_SYS_{name} == UINT64_C({s["number"]}), "{name}");'
                    for name, s in self.calls.items())
                subprocess.run([compiler, "-target", target, "-ffreestanding", "-fsyntax-only",
                                "-pedantic-errors", "-Wall", "-Werror", "-std=" + standard,
                                "-I", abi.ABI / "generated", "-x", language, "-"],
                               input=test + "\n", text=True, check=True)


if __name__ == "__main__":
    unittest.main()
