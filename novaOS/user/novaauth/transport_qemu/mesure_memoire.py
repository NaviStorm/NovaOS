"""Mesure du type réellement utilisé par le processus natif de transport."""
import json
import os
import subprocess

if __name__ == "__main__":
    result = subprocess.check_output([os.environ["NOVA_ADAPTATEUR"], "--mesure"], text=True)
    print(result, end="")
    m = json.loads(result)
    assert m["total_octets"] == m["pages_utiles"] + m["supplement"]
    raise SystemExit(0 if max(m["supplement"], m["serialise"]) <= 256 else 1)
