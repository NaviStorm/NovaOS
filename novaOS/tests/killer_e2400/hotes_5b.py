"""Rejoue les tests E2400, le gestionnaire et les témoins VirtIO-net/NVMe sur les deux ISA Darwin."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib

HERE = Path(__file__).resolve().parent
NOVA = HERE.parents[1]
BASE = "7c3bb99a2f63326818024ac8e498ec329c6ad345"


def output(*args):
    return subprocess.check_output(args, cwd=NOVA.parent, text=True).strip()


def run(cmd, log, env):
    with log.open("xb") as stream:
        subprocess.run(cmd, cwd=NOVA.parent, env=env, check=True, stdout=stream, stderr=subprocess.STDOUT)
    data = log.read_bytes()
    counts = [int(n) for n in re.findall(rb"test result: ok\. (\d+) passed; 0 failed", data)]
    if not counts or sum(counts) == 0:
        raise ValueError(f"Suite incomplète {log}")
    return {"tests":sum(counts),"groupes":counts,"sha256":hashlib.sha256(data).hexdigest()}


def main():
    cli=argparse.ArgumentParser(description=__doc__)
    cli.add_argument("sortie",type=Path)
    args=cli.parse_args()
    dest=args.sortie.resolve()
    dest.mkdir(parents=True,exist_ok=False)
    cargo=["rustup","run","stable","cargo","test","--release","--locked","--offline"]
    rustc=output("rustup","which","--toolchain","stable","rustc")
    env={**os.environ,"CARGO_TARGET_DIR":str(dest/"cibles"),"RUSTC":rustc,"RUSTDOC":output("rustup","which","--toolchain","stable","rustdoc")}
    protected=["novaOS/abi","novaOS/kernel","novaOS/Cargo.toml","novaOS/user/ethernet","novaOS/user/virtio_net",
               "novaOS/user/gestionnaire-4d","novaOS/user/novaauth","novaOS/libs/catalogue-pilotes","novaFS"]
    output("git","diff","--exit-code","--quiet",BASE,"--",*protected)
    abi=len(tomllib.loads((NOVA/"abi/syscalls.toml").read_text())["call"])
    if abi!=48:
        raise ValueError("Nombre de sélecteurs")
    meta={"base":BASE,"revision":output("git","rev-parse","HEAD"),"rustc":output(rustc,"-Vv"),"abi":abi,"suites":{}}
    for isa,arch in (("aarch64","arm64"),("x86_64","x86_64")):
        triple=isa+"-apple-darwin"
        translated=output("arch","-"+arch,"/usr/sbin/sysctl","-in","sysctl.proc_translated")
        if translated not in (("0","") if arch=="arm64" else ("1",)) or output("arch","-"+arch,"uname","-m")!=arch:
            raise ValueError("Mode d’exécution hôte inattendu")
        meta["suites"][isa]=suites={"machine":arch,"rosetta":translated=="1"}
        for name,manifest in (("e2400",HERE/"Cargo.toml"),("gestionnaire",NOVA/"libs/gestionnaire-pilotes/Cargo.toml")):
            log=dest/f"{name}-{isa}.log"
            suites[name]=run([*cargo,"--target",triple,"--manifest-path",str(manifest),"--","--test-threads=1","--nocapture"],log,env)
            data=log.read_text()
            if name=="e2400" and any(f"test result: ok. {n} passed;" not in data for n in (21,12,7)):
                raise ValueError("Suite 5A absente")
            if name=="gestionnaire" and ("endurance=256 delta=0" not in data or "dma_frames=259 heap=777" not in data):
                raise ValueError("Mesure du gestionnaire absente")
            suites[name]["mesures"]=[line.split(" ... ")[-1] for line in data.splitlines() if any(
                key in line for key in ("horloge_cpu_sonde=","dma_frames=","courtier_tete=","courtier_264_allers_retours_cpu_ns","gestionnaire_tete=","endurance=256 delta="))]
        for name,source in (("virtio-net",NOVA/"user/virtio_net/mod.rs"),("nvme",NOVA/"tests/nvme_controleur.rs"),
                             ("nvme-diagnostic",NOVA/"tests/nvme_diagnostic.rs")):
            binary=dest/f"{name}-{isa}"
            subprocess.run([rustc,"--edition=2024","--test","-O","--target",triple,str(source),"-o",str(binary)],check=True,env=env)
            suites[name]=run([str(binary),"--test-threads=1"],dest/f"{name}-{isa}.log",env)
        print(isa,{k:v["tests"] for k,v in suites.items() if isinstance(v,dict)},flush=True)
    (dest/"hotes.json").write_text(json.dumps(meta,ensure_ascii=False,indent=2)+"\n")


if __name__=="__main__":
    main()
