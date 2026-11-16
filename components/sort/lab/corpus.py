"""Corpus dense à enregistrements fixes, reproductible sans dépendance."""
import argparse
import hashlib
import json
import shutil
import time
from pathlib import Path

WIDTH = 4096
SEED = 27
RESERVE = 16 << 30


def generate(path, size, distribution):
    if size <= 0 or size % WIDTH:
        raise ValueError("la taille doit être un multiple positif de 4096")
    free = shutil.disk_usage(path.parent).free
    if free < 3 * size + RESERVE:
        raise ValueError(f"espace insuffisant : {free} octets libres")
    rows = size // WIDTH
    digest = hashlib.sha256()
    state = SEED
    pad = b"x" * (WIDTH - 35)
    start = time.monotonic()
    with path.open("xb") as stream:
        for block in range(0, rows, 1024):
            if block % 262144 == 0 and shutil.disk_usage(path.parent).free < RESERVE:
                raise OSError("marge disque atteinte")
            data = bytearray()
            for i in range(block, min(rows, block + 1024)):
                state = (state * 6364136223846793005 + 1442695040888963407) & ((1 << 64) - 1)
                key = {"random": state, "sorted": i, "reverse": rows-i, "duplicates": state % 17}[distribution]
                data.extend(f"{key:016x} {i:016x} ".encode())
                data.extend(pad)
                data.append(10)
            stream.write(data)
            digest.update(data)
    stat = path.stat()
    assert stat.st_size == size and stat.st_blocks * 512 >= size
    return dict(seed=SEED, width=WIDTH, rows=rows, size=size,
                allocated=stat.st_blocks * 512, input_sha256=digest.hexdigest(),
                distribution=distribution, generation_s=time.monotonic()-start,
                initial_free=free, reserve=RESERVE, path=str(path))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("path", type=Path)
    parser.add_argument("size", type=int)
    parser.add_argument("distribution", choices=["random", "sorted", "reverse", "duplicates"])
    args = parser.parse_args()
    print(json.dumps(generate(args.path, args.size, args.distribution), indent=2))
