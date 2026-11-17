"""Contrôler les flux et le statut du candidat avec stdout sur fichier, pipe et /dev/null."""
import json
import os
from pathlib import Path
import subprocess
import tempfile

from laboratoire import TMP, parser, tools

args = parser(__doc__).parse_args()
bins = tools(args)
observations = []
with tempfile.TemporaryDirectory(dir=TMP, prefix="cmp-sortie-") as tmp:
    work = Path(tmp)
    (work / "a").write_bytes(b"ab")
    (work / "b").write_bytes(b"a")
    env = {**os.environ, "LC_ALL": "C", "LANG": "C"}
    for tool, cmp in bins.items():
        for dest in ("fichier", "pipe", "null"):
            for flags, rc in [(["a", "a"], 0), (["a", "b"], 1), (["-s", "absent", "b"], 2)]:
                with (work / "sortie").open("wb") as f:
                    stdout = {"fichier": f, "pipe": subprocess.PIPE, "null": subprocess.DEVNULL}[dest]
                    p = subprocess.run(["cmp", *flags], executable=cmp, cwd=work, env=env,
                                       stdout=stdout, stderr=subprocess.PIPE, check=False)
                out = (work / "sortie").read_bytes() if dest == "fichier" else (p.stdout or b"")
                err = b"cmp: EOF on 'b' after byte 1, in line 1\n" if rc == 1 and dest != "null" else b""
                observations.append(dict(outil=tool, destination=dest, arguments=flags,
                                         statut=p.returncode, stdout=out.hex(), stderr=p.stderr.hex()))
                assert (p.returncode, out, p.stderr) == (rc, b"", err), (tool, dest, flags, p)
        print(f"{tool} : 9 contrôles de stdout vérifiés")
if args.json:
    args.json.write_text(json.dumps(observations, indent=2) + "\n")
