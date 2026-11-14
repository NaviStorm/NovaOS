"""Exécute les suites de découverte et de retrait des ressources sans disque invité."""
import json
from pathlib import Path
import platform
import re
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
BUILD = ROOT / "build"
RUSTC = ["rustup", "run", "stable", "rustc"]
SUITES = (
    "kernel/src/memory/firmware_pages.rs",
    "kernel/src/hardware/acpi.rs",
    "kernel/src/hardware/virt_dtb.rs",
    "kernel/src/hardware/pci_inventory.rs",
    "kernel/src/hardware/driver_resources.rs",
    "user/device_inventory.rs",
    "user/device_lifecycle.rs",
)
RESULT = re.compile(r"^test result: ok\. (\d+) passed; 0 failed; 0 ignored; "
                    r"0 measured; 0 filtered out;", re.MULTILINE)


def profiles(host):
    if platform.system() == "Darwin" and (
        platform.machine() == "arm64" or host == "aarch64-apple-darwin"
        or subprocess.run(["/usr/bin/arch", "-arm64", "/usr/bin/true"],
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
    ):
        return [
            ("Darwin ARM64 natif", "aarch64-apple-darwin", ["/usr/bin/arch", "-arm64"]),
            ("Darwin x86_64 Rosetta", "x86_64-apple-darwin", ["/usr/bin/arch", "-x86_64"]),
        ]
    return [(f"Hôte {host}", host, [])]


def main():
    version = subprocess.check_output([*RUSTC, "-vV"], text=True)
    host = next(s.removeprefix("host: ") for s in version.splitlines() if s.startswith("host: "))
    print(version.splitlines()[0], flush=True)
    BUILD.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="modeles-materiel-", dir=BUILD) as folder:
        for label, target, launcher in profiles(host):
            total = 0
            for suite in SUITES:
                name = Path(suite).stem
                binary = Path(folder) / f"{target}-{name}"
                # Conserver les visibilités pub(super) et les chemins des sous-modules.
                source = Path(folder) / f"{name}.rs"
                source.write_text(f"#[path = {json.dumps(str(ROOT / suite), ensure_ascii=False)}] pub mod {name};\n")
                subprocess.run([*RUSTC, "--edition", "2024", "--test", "--target", target,
                                source, "-o", binary], check=True)
                run = subprocess.run([*launcher, binary], stdout=subprocess.PIPE,
                                     stderr=subprocess.STDOUT, text=True)
                counts = RESULT.findall(run.stdout)
                if run.returncode or len(counts) != 1 or int(counts[0]) == 0:
                    raise SystemExit(f"{label}, suite {name} en échec.\n{run.stdout}")
                total += int(counts[0])
                print(f"{label} — {name} — {counts[0]} réussis", flush=True)
            print(f"{label} — total {total} réussis dans {len(SUITES)} suites", flush=True)


if __name__ == "__main__":
    main()
