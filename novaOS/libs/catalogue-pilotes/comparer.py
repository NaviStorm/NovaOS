"""Rejoue le témoin 4B et la version courante, avec le même banc hors ligne."""
from hashlib import sha256
from pathlib import Path
import os
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parent
REPO = ROOT.parents[2]
REL = ROOT.relative_to(REPO)
REF = "0eea6bc694533afc4ab037d08b6c40cf28e1103b"
BUILD = Path(os.environ["CARGO_TARGET_DIR"]).resolve()
if not BUILD.is_relative_to(Path(os.environ["NOVAOS_TMPDIR"]).resolve()):
    sys.exit("Répertoire de compilation privé requis")

def run(cmd, directory, env=None):
    return subprocess.check_output(cmd, cwd=directory, env=env, text=True)

def measure(label, crate, build):
    env = dict(os.environ, CARGO_TARGET_DIR=str(build), CARGO_PROFILE_RELEASE_STRIP="none")
    print(f"=== {label} ===", flush=True)
    subprocess.run([sys.executable, "-B", crate / "qualifier.py"], cwd=crate, env=env, check=True)
    results = []
    for isa in ["aarch64-apple-darwin", "x86_64-apple-darwin"]:
        log = run(["rustup", "run", "stable", "cargo", "test", "--offline", "--locked", "--release",
                   "--target", isa, "--test", "plans", "--", "--nocapture", "--test-threads=1"], crate, env)
        lines = [line[line.index("PLAN|"):] for line in log.splitlines() if "PLAN|" in line]
        assert len(lines) == 26
        print(isa, len(lines), sha256(("\n".join(lines)+"\n").encode()).hexdigest(), flush=True)
        results.append(lines)
    assert results[0] == results[1]
    return results[0]

BUILD.mkdir(parents=True, exist_ok=True)
with tempfile.TemporaryDirectory(dir=BUILD) as tmp:
    snapshot = Path(tmp)
    archive = snapshot / "reference.tar"
    subprocess.run(["git", "archive", "-o", archive, REF, "--", REL,
                    "novaOS/user/novaauth", "novaFS", "novaOS/.cargo"], cwd=REPO, check=True)
    with tarfile.open(archive) as tar:
        tar.extractall(snapshot, filter="data")
    old = snapshot / REL
    for name in ["qualifier.py", "tests/plans.rs", "examples/support/maximal.rs"]:
        shutil.copyfile(ROOT / name, old / name)
    # Seule cette ligne du banc expose un compteur absent de l'API 4B.
    bench = Path("src/selection/tests/mesures.rs")
    source = (ROOT / bench).read_text()
    marker = 'std::println!("index_objets n={n} {:?}", catalog.memory_bytes());'
    assert source.count(marker) == 1
    (old / bench).write_text(source.replace(marker, ""))
    before = measure("4B " + REF, old, BUILD / "temoin")
    after = measure("4B.1 " + run(["git", "rev-parse", "HEAD"], ROOT).strip(), ROOT, BUILD)
    assert before == after, "Les plans ou les refus diffèrent entre révisions"
    print("26 plans ou décisions identiques entre révisions et architectures", flush=True)
