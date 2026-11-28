"""Mesures séquentielles sur images régulières, SHA-256 de tous les octets logiques."""

import argparse
import json
from pathlib import Path
import resource
import time
from image5o import MIO, chargeur, copier, creer, lecteur, sha, verifier, require


def mesurer(action, *args):
    t = time.perf_counter_ns()
    out = action(*args)
    return out, (time.perf_counter_ns() - t) // 1000


def campagne(out):
    out.mkdir(mode=0o700)
    pe = chargeur()
    rows = []
    for m in (68, 1024, 8192, 16384):
        n = m * MIO
        p, second, dest = [out / f"{m}-{s}.img" for s in ("origine", "bis", "media")]
        _, creation = mesurer(creer, p, n, pe)
        creer(second, n, pe)
        with lecteur(p) as f:
            info, controle = mesurer(verifier, f, n, pe)
            origin, temps_sha = mesurer(sha, f)
        with lecteur(second) as f:
            require(sha(f) == origin, "Reconstruction divergente")
        inst, copie = mesurer(copier, p, dest, n, pe)
        with lecteur(dest) as f:
            final = sha(f)
        require(final == origin, "Copie divergente")
        rows.append(
            dict(
                **info,
                sha256=origin,
                sha_copie=final,
                octets_alloues_copie=inst["alloues"],
                generation_us=creation,
                controle_us=controle,
                copie_controles_us=copie,
                sha_us=temps_sha,
            )
        )
    return dict(
        capacites=rows,
        pic_rss_octets_darwin=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("dossier", type=Path)
    print(json.dumps(campagne(p.parse_args().dossier), indent=2))
