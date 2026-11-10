#!/usr/bin/env python3
"""Vérifie les blocs LZ4 du moteur avec la bibliothèque de référence du laboratoire macOS."""
import argparse
import ctypes
import ctypes.util
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import tempfile

LAB = Path(__file__).resolve().parent
ARCHS = ("aarch64", "x86_64")
SEED = 0x6E6F766146535036


def exemples():
    rng = random.Random(SEED)
    for i in range(256):
        noise = rng.randbytes(4096)
        if i % 4 == 0:
            yield noise
        elif i % 4 == 1:
            pattern = noise[:1 + i % 128]
            yield (pattern * (4096 // len(pattern) + 1))[:4096]
        elif i % 4 == 2:
            n = rng.randrange(1, 4096)
            yield noise[:n] + bytes([noise[0]]) * (4096 - n)
        else:
            # Motifs copiés à plusieurs distances, comprenant des chevauchements.
            page = bytearray(noise)
            for _ in range(20):
                start = rng.randrange(1, 4096)
                offset = rng.randrange(1, start + 1)
                for j in range(start, min(4096, start + rng.randrange(1, 700))):
                    page[j] = page[j - offset]
            yield bytes(page)


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--bibliotheque", help="chemin vers liblz4 de référence")
    args = ap.parse_args()
    library = args.bibliotheque or ctypes.util.find_library("lz4")
    if not library:
        library = Path("/opt/homebrew/opt/lz4/lib/liblz4.dylib")
    lib = ctypes.CDLL(str(library))
    lib.LZ4_versionString.restype = ctypes.c_char_p
    for name in ("LZ4_compress_default", "LZ4_decompress_safe"):
        getattr(lib, name).argtypes = (ctypes.c_char_p, ctypes.c_void_p, ctypes.c_int, ctypes.c_int)
        getattr(lib, name).restype = ctypes.c_int
    cases = list(exemples())
    result = dict(bibliotheque=lib.LZ4_versionString().decode(), graine=hex(SEED), architectures={})
    with tempfile.TemporaryDirectory(prefix="novafs-p6-lz4-") as work:
        work = Path(work)
        source = work / "source"
        with source.open("wb") as f:
            output = ctypes.create_string_buffer(lib.LZ4_compressBound(4096))
            for raw in cases:
                n = lib.LZ4_compress_default(raw, output, len(raw), len(output))
                if n <= 0:
                    raise ValueError("échec de compression de référence")
                f.write(raw + n.to_bytes(2, "little") + output.raw[:n])
        corpus = source.read_bytes()
        result["sha256_corpus"] = hashlib.sha256(corpus).hexdigest()
        for arch in ARCHS:
            folder = work / arch
            folder.mkdir()
            (folder / "source").write_bytes(corpus)
            proc = subprocess.run(
                [LAB / "run", "test", "--offline", "--lib", "compression::tests::corpus_liblz4_bidirectionnel",
                 "--", "--ignored", "--exact"],
                env=dict(os.environ, NOVAFS_TARGET=f"{arch}-apple-darwin", NOVAFS_LZ4_FIXTURE=str(folder)),
                check=True, stdout=subprocess.PIPE, text=True)
            if "1 passed; 0 failed" not in proc.stdout:
                raise ValueError("test Rust d'interopérabilité absent")
            reply = folder / "reponse"
            with reply.open("rb") as f:
                buf = ctypes.create_string_buffer(4096)
                for expected in cases:
                    length = f.read(2)
                    if len(length) != 2:
                        raise ValueError("réponse NovaFS incomplète")
                    n = int.from_bytes(length, "little")
                    block = f.read(n)
                    if len(block) != n or lib.LZ4_decompress_safe(block, buf, n, 4096) != 4096:
                        raise ValueError("le décodeur de référence refuse le bloc NovaFS")
                    if buf.raw != expected:
                        raise ValueError("divergence entre NovaFS et le décodeur de référence")
                if f.read(1):
                    raise ValueError("flux de réponse résiduel")
            result["architectures"][arch] = dict(
                reference_vers_novafs=len(cases), novafs_vers_reference=len(cases),
                sha256_flux_novafs=hashlib.sha256(reply.read_bytes()).hexdigest())
    if result["architectures"][ARCHS[0]] != result["architectures"][ARCHS[1]]:
        raise ValueError("encodage dépendant de l'architecture")
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
