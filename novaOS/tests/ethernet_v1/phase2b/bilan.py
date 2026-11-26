"""Vérifie les séries de trafic et de retrait Ethernet 2B puis régénère les résultats."""

import argparse
import gzip
import itertools
import json
import tomllib
from fractions import Fraction
from pathlib import Path
from statistics import median

ARCHS = ("aarch64", "x86_64")
NOMS = ("DATA", "Boîte", "Fenêtres")
PROFILS = ("Rafale", "Rare", "Saturation", "Concurrent")
GEOMETRIES = [(0, b, l) for b, l in itertools.product((1, 4, 8), (60, 1514))]
GEOMETRIES += [(1, 1, 60), (1, 1, 1514), (2, 8, 1514), (3, 1, 60)]
MEMOIRE = (
    "tables",
    "kpile",
    "upile",
    "heap",
    "buddy",
    "ipc",
    "utilisateur",
    "dma",
    "elf_preparation",
)


def lignes(path):
    data = path.read_bytes()
    if path.suffix == ".gz":
        data = gzip.decompress(data)
    return data.decode().splitlines()


def champs(ligne):
    return {
        k: int(v)
        for mot in ligne.split()
        if "=" in mot
        for k, v in [mot.split("=", 1)]
        if v.isdecimal()
    }


def rang(trie, millieme):
    return trie[(millieme * len(trie) + 999) // 1000 - 1]


def resumer(c, r, brut, med, ram, rare_long):
    n, hz = c["n"], c["hz"]
    b, l, m, p = (c[k] for k in ("b", "len", "mode", "profil"))
    assert n == r["n"] == len(brut) == (1000 if p == 1 and not rare_long else 10000)
    assert min(brut) > 0 and sum(brut) <= r["ticks"] and r["solde"] == 0
    volume = b * l
    pages = (volume + 4095) // 4096
    frags = b * ((l + 95) // 96)
    units = frags if m == 0 else pages
    assert r["copies"] == (4 * frags if m == 0 else 2 * pages)
    assert r["octets"] == (4 if m == 0 else 2) * volume
    assert r["pauses"] == (n if p == 1 else 0)
    assert r["pression"] >= (n if p == 2 else 0) and r["wakes"] > 0
    assert (
        r["appels"]
        == n * (4 + 2 * units + 4 * (p == 2)) + r["pression"] + r["reprises"] + r["pauses"]
    )
    assert (r["voisin"] > 0) == (p == 3)
    banques = (0, 1, pages)[m]
    assert r["p"] == 2 + 2 * (p == 3) and r["boites"] == banques + (p == 3)
    assert r["ep"] == 2 + max(0, banques - 1) + (p == 2) + 2 * (p == 3)
    assert set(ram) == {"base", "attente"}
    diff = {k: ram["attente"][k] - ram["base"][k] for k in MEMOIRE}
    assert diff["dma"] == diff["elf_preparation"] == 0
    assert (
        sum(diff.values())
        == r["cadres_attente"]
        == ram["base"]["libres"] - ram["attente"]["libres"]
    )
    assert diff["kpile"] == 16 * r["p"]
    assert all(
        ram["base"][k] == ram["attente"][k]
        for k in ("image", "video", "ram_video", "cache", "chargeur", "extra_elf")
    )
    assert diff["ipc"] - r["ipc"] == r["cadres_attente"] - r["cadres_ouverts"]
    assert diff["ipc"] > r["ipc"]
    assert (r["priva"], r["privb"]) == (6 + pages, 6 + 2 * pages)
    assert r["pilea"] == r["pileb"] == 64
    assert (
        diff["upile"] + diff["utilisateur"]
        == r["priva"] + r["pilea"] + r["privb"] + r["pileb"] + r["privvoisin"]
    )
    assert set(med) == (set(range(4)) if m else set())
    if m:
        assert med[0]["appels"] == med[2]["appels"] == 128 * pages
    tries = sorted(brut)
    quantiles = {str(q / 10): rang(tries, q) for q in (500, 950, 990, 999)}
    return dict(
        **c,
        brut_compteurs=r,
        ram_base=ram["base"],
        ram_attente=ram["attente"],
        pages_par_poste=diff,
        mediation_128_lots=med,
        ticks_cumules=sum(brut),
        latence_ticks=quantiles,
        latence_ns={k: v * 1_000_000_000 / hz for k, v in quantiles.items()},
        utile_Mo_s=volume * n * hz / (1e6 * sum(brut)),
        cadence_Mo_s=volume * n * hz / (1e6 * r["ticks"]),
        appels_par_lot=r["appels"] / n,
        commutations_par_lot=r["switches"] / n,
        reveils_par_lot=r["wakes"] / n,
        duree_retrait_us=r["retrait"] * 1e6 / hz,
    )


def lire_serie(path, boot, rare_long=False):
    rows = {}
    c = r = None
    samples = []
    med = {}
    ram = {}
    rotation = None
    complete = 0

    def terminer():
        if c is None:
            return
        assert r is not None
        key = tuple(c[k] for k in ("profil", "b", "len", "mode"))
        assert key not in rows
        rows[key] = resumer(c, r, samples, med, ram, rare_long)

    for line in lignes(path):
        assert "Panique du noyau" not in line
        if line.startswith("ETH2B ordre "):
            rotation = champs(line)["rotation"]
            assert rotation == boot - 1
        elif line.startswith("RAM phase=eth2b_base "):
            terminer()
            ram = {"base": champs(line)}
            c = r = None
            samples = []
            med = {}
        elif line.startswith("RAM phase=eth2b_attente "):
            ram["attente"] = champs(line)
        elif line.startswith("ETH2B debut "):
            assert c is None
            c = champs(line)
        elif line.startswith("ETH2B mediation "):
            v = champs(line)
            assert v["type"] not in med
            med[v["type"]] = v
        elif line.startswith("ETH2B resultat "):
            assert r is None
            r = champs(line)
        elif line.startswith("ETH2B serie "):
            record = line.split()
            assert int(record[2]) == len(samples)
            samples.extend(int(t, 16) for t in record[3:])
        elif line.startswith("ETH2B campagne OK"):
            complete += 1
    terminer()
    assert rotation == boot - 1 and complete == 1
    geometries = [g for g in GEOMETRIES if g[0] == 1] if rare_long else GEOMETRIES
    assert set(rows) == {(*g, m) for g in geometries for m in range(3)}
    for g in geometries:
        expected = [(i + sum(g) + rotation) % 3 for i in range(3)]
        assert [k[3] for k in rows if k[:3] == g] == expected
    return rows


def lire_surete(path):
    lines = lignes(path)
    assert "ETH2B campagne OK" in lines and not any("Panique du noyau" in line for line in lines)
    records = [champs(line) for line in lines if line.startswith("ETH2B surete ")]
    assert [r["cas"] for r in records] == [4, 5, 6, 7, 8]
    for r in records:
        assert r["admis"] == int(r["cas"] == 7) and r["croisement_refuse"] == 2
        assert r["premiere_page"] == 1 and r["voisin"] > 0 and r["solde"] == 0
    return records


def lire_course(path):
    registre = Path(__file__).resolve().parents[3] / "abi/syscalls.toml"
    appels = tomllib.loads(registre.read_text())["call"]
    assert len(appels) == 48

    def numero(nom):
        return appels["IPC_" + nom]["number"]

    attentes = [
        (numero("TAKE"), numero("CLOSE")),
        (numero("TAKE"), numero("REVOKE")),
        (numero("PUT"), numero("CLOSE")),
    ]
    lines = lignes(path)
    assert "ETH2B campagne OK" in lines and not any("Panique du noyau" in line for line in lines)
    records = [champs(line) for line in lines if line.startswith("ETH2B course ")]
    assert [(r["copie"], r["retrait"]) for r in records] == attentes
    for r in records:
        assert {r["cpu_copie"], r["cpu_retrait"]} == {0, 1}
        assert (r["premiere"], r["contention"], r["fin"], r["anciens_refuses"], r["solde"]) == (
            2048,
            1,
            4096,
            2,
            0,
        )
    return records


def decision(rows):
    def debit(b, duree_totale=False):
        vitesses = []
        for mode in range(3):
            r = rows[0, b, 1514, mode]
            ticks = r["brut_compteurs"]["ticks"] if duree_totale else r["ticks_cumules"]
            vitesses.append(Fraction(b * 1514 * r["n"] * r["hz"], ticks))
        return 100 * (vitesses[2] / max(vitesses[:2]) - 1)

    def p99(longueur):
        cellules = [rows[0, 1, longueur, mode] for mode in range(3)]
        temps = [Fraction(r["latence_ticks"]["99.0"], r["hz"]) for r in cellules]
        return 100 * (temps[2] / min(temps[:2]) - 1)

    gains = [debit(b) for b in (4, 8)]
    ecarts = [p99(l) for l in (60, 1514)]
    return dict(
        gain_debit_pct=list(map(float, gains)),
        variation_p99_pct=list(map(float, ecarts)),
        gain_cadence_pct=[float(debit(b, duree_totale=True)) for b in (4, 8)],
        conforme=min(gains) >= 10 and max(ecarts) <= 10,
    )


def fmt(x, n=1):
    return f"{x:.{n}f}".replace(".", ",")


def publier(series, verdicts, rare):
    out = [
        "# Ethernet 2B — mesures en invité",
        "",
        "Débits en Mo/s décimaux. La décision utilise le volume utile divisé par la somme des durées d’admission, comme en phase 2. La cadence utilise toute la durée entre le premier envoi et le dernier reçu, intervalles et travail utilisateur compris. Chaque quantile emploie le rang supérieur sur les ticks individuels dans leur série.",
        "",
        "| ISA | Démarrage | Gain débit fenêtres B=4 / B=8, 1 514 octets | Variation p99 fenêtres B=1, 60 / 1 514 octets | Quatre seuils |",
        "|---|---:|---:|---:|---|",
    ]
    for (isa, boot), d in verdicts.items():
        gains = " / ".join(fmt(x) + " %" for x in d["gain_debit_pct"])
        tails = " / ".join(fmt(x) + " %" for x in d["variation_p99_pct"])
        out.append(
            f"| {isa} | {boot} | {gains} | {tails} | "
            + ("atteints" if d["conforme"] else "non atteints")
            + " |"
        )
    for isa in ARCHS:
        out += [
            "",
            "## " + isa,
            "",
            "Chaque ligne résume trois démarrages indépendants. Les colonnes chiffrées donnent la médiane des trois valeurs par série. Le fichier JSON conserve chaque démarrage, le décompte des appels refusés et les photos de mémoire. Les durées des PUT/TAKE incluent les vérifications et les copies dans le gestionnaire. Elles proviennent d’une passe séparée de 128 lots.",
            "",
            "| Profil | B | Octets | Transport | Lots/série | Débit / cadence (Mo/s) | p50 / p95 / p99 / p99,9 (µs) | IPC / commut. / réveils par lot | Copies / octets copiés par lot | PUT / TAKE réussi (µs) | Cadres en attente |",
            "|---|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|",
        ]
        for p, b, l in GEOMETRIES:
            for m in range(3):
                rs = [series[isa, i][p, b, l, m] for i in (1, 2, 3)]

                def stat(key):
                    return median(r[key] for r in rs)

                def raw(key):
                    return median(r["brut_compteurs"][key] for r in rs)

                times = [
                    median(r["latence_ns"][key] for r in rs) / 1000
                    for key in ("50.0", "95.0", "99.0", "99.9")
                ]

                def cout(i):
                    return median(
                        r["mediation_128_lots"][i]["ticks"]
                        * 1e6
                        / (r["hz"] * r["mediation_128_lots"][i]["appels"])
                        for r in rs
                    )

                mediation = " / ".join(fmt(cout(i), 2) for i in (0, 2)) if m else "s.o."
                out.append(
                    "| "
                    + " | ".join(
                        [
                            PROFILS[p],
                            str(b),
                            str(l),
                            NOMS[m],
                            str(rs[0]["n"]),
                            " / ".join(fmt(stat(k), 2) for k in ("utile_Mo_s", "cadence_Mo_s")),
                            " / ".join(fmt(v, 2) for v in times),
                            " / ".join(
                                fmt(stat(k), 2)
                                for k in (
                                    "appels_par_lot",
                                    "commutations_par_lot",
                                    "reveils_par_lot",
                                )
                            ),
                            f"{raw('copies')} / {raw('octets')}",
                            mediation,
                            str(raw("cadres_attente")),
                        ]
                    )
                    + " |"
                )
    out += [
        "",
        "## Trafic rare à 10 000 lots par série",
        "",
        "Trames unitaires de 60 et 1 514 octets. Trois démarrages additionnels par ISA. Chaque colonne de latence donne le minimum et le maximum des quantiles obtenus sur les trois séries. La cadence inclut l’attente d’un tick entre lots.",
        "",
        "| ISA | Octets | Transport | p50 (µs) | p95 (µs) | p99 (µs) | p99,9 (µs) | Cadence médiane (Mo/s) |",
        "|---|---:|---|---:|---:|---:|---:|---:|",
    ]
    for isa in ARCHS:
        for l, m in itertools.product((60, 1514), range(3)):
            rs = [rare[isa, i][1, 1, l, m] for i in (1, 2, 3)]
            spans = []
            for key in ("50.0", "95.0", "99.0", "99.9"):
                v = [r["latence_ns"][key] / 1000 for r in rs]
                spans.append(fmt(min(v), 2) + "–" + fmt(max(v), 2))
            out.append(
                "| "
                + " | ".join(
                    [isa, str(l), NOMS[m], *spans, fmt(median(r["cadence_Mo_s"] for r in rs), 4)]
                )
                + " |"
            )
    return "\n".join(out) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preuves", type=Path)
    parser.add_argument("markdown", type=Path)
    parser.add_argument("json", type=Path)
    args = parser.parse_args()
    if not __debug__:
        parser.error("Les vérifications exigent Python sans optimisation (-O).")
    series = {}
    decisions = {}
    rare = {}
    detail = {}
    for isa in ARCHS:
        boots = []
        rare_boots = []
        for boot in (1, 2, 3):
            rows = lire_serie(args.preuves / f"qemu-{isa}-{boot}.log.gz", boot)
            series[isa, boot] = rows
            d = decision(rows)
            decisions[isa, boot] = d
            boots.append(dict(demarrage=boot, decision=d, cellules=list(rows.values())))
            extra = lire_serie(args.preuves / f"rare-{isa}-{boot}.log.gz", boot, rare_long=True)
            rare[isa, boot] = extra
            rare_boots.append(dict(demarrage=boot, cellules=list(extra.values())))
        detail[isa] = dict(
            surete=lire_surete(args.preuves / f"surete-{isa}.log.gz"),
            course_smp=lire_course(args.preuves / f"course-{isa}.log.gz"),
            demarrages=boots,
            trafic_rare_long=rare_boots,
        )
    args.markdown.write_text(publier(series, decisions, rare))
    args.json.write_text(json.dumps(detail, ensure_ascii=False, indent=2) + "\n")


if __name__ == "__main__":
    main()
