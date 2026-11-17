"""Recette différentielle : arguments, statut et deux flux binaires pour chaque appel."""

import argparse
import itertools
import random
import signal
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
GNU = Path("/opt/homebrew/bin/gprintf")
NOVA = HERE / "bin/printf"
TMP = HERE.parent.parent / "tmp/printf-0240"
LOCALES = ["C", "en_US.UTF-8", "fr_FR.UTF-8"]


def corpus():
    for name, args in {
        "absent": [],
        "separateur-seul": ["--"],
        "aide": ["--help"],
        "version": ["--version"],
        "aide-format": ["--help", "x"],
        "version-format": ["--version", "y"],
        "aide-apres-separateur": ["--", "--help"],
        "version-apres-separateur": ["--", "--version"],
        "separateur-format": ["--", "--"],
        "option-donnee": ["%s|", "--help", "--version", "--"],
        "options-courtes": ["-h"],
        "version-courte": ["-V"],
        "abreviation": ["--hel"],
        "deux-options": ["--help", "--version"],
        "deux-options-inverse": ["--version", "--help"],
        "option-inconnue": ["--autre"],
        "vide": [""],
        "vide-exces": ["", ""],
        "reprise": ["<%s%d>", "un", "3", "deux"],
        "manquants": ["%d-%a-%s-%b-%q-%c"],
        "position": ["%2$s/%s/%s/%1$s;", "A", "B", "C"],
        "etoiles": ["%3$*1$.*2$f", "9", "3", "3.125"],
        "melange": ["%2$*s:%.*1$s", "5", "bonjour", "fin"],
        "position-bq": ["%2$b%1$q", "a b", r"\0"],
        "index-grand": ["%999999999999999999999999999999$s", "a"],
        "index-zero": ["%0$s", "a"],
        "largeur-negative": ["%*.*s", "-8", "3", "abcdef"],
        "precision-negative": ["%.*d", "-9000000000", "3"],
        "largeur-hors-borne": ["%*s", "2147483648", "abc"],
        "precision-hors-borne": ["%.*d", "2147483648"],
        "etoile-invalide": ["%*s", "", "abc"],
        "octal-format": [r"\0|\01|\001|\0001|\777|\400"],
        "octal-b": ["%b", r"\0|\01|\001|\0001|\777|\0400"],
        "hex": [r"\x1\x12\x1AB"],
        "barres": [b'\\"\\\\\\a\\b\\e\\f\\n\\r\\t\\v\\!\\'],
        "barres-b": ["%b", b'\\"\\\\\\a\\b\\e\\f\\n\\r\\t\\v\\!\\'],
        "annulation-format": [r"A\c%Z"],
        "annulation-argument": ["%b%Z", r"A\cZ"],
        "annulation-apres-erreur": [r"%d\c", "oops"],
        "unicode": [r"\u0000\u0024\u00e9\U0001F642\U00110000"],
        "surrogate": [r"ok\udfff"],
        "hex-vide": [r"A\xQ"],
        "unicode-tronque": [r"\U001"],
        "pourcent": ["%%"],
        "sans-lf": ["texte"],
        "percent-incomplet": ["debut%"],
        "premier-octet": ["%c", "é"],
        "chaine-brute": [b"%.2s", b"\x80\xff\xfe"],
        "formats-bruts": [b"\xff%s\x80", b"\x89"],
        "grande-largeur": ["%500000s", "."],
        "grande-precision": ["%.70000f", "1.125"],
        "invalide-apres-sortie": ["OK%s%n", "X"],
        "constante": ["%i|", "'é", '"Axyz', b"'\xff", "'", "'xy"],
        "entiers-limites": [
            "%d|",
            "-9223372036854775809",
            "9223372036854775807",
            "9223372036854775808",
            "09",
            "0b11",
            "-0x12",
            "  +07",
            "-",
        ],
        "non-signes": [
            "%u|",
            "-1",
            "18446744073709551615",
            "18446744073709551616",
            "-18446744073709551616",
        ],
        "groupement": ["%'015.9d %'14.3f %'.8g", "1234567", "34567.25", "12345.25"],
        "locale-numerique": ["%g|", "2.5", "2,5", "1.2,3", "1,2.3"],
        "chiffre-local-I": ["%Id", "1"],
        "nul-c": ["%c", ""],
    }.items():
        yield name, args
    yield "precision-litterale-64", ["%.2147483649s", "x"]
    yield "largeur-debordement-64", ["A%999999999999999999999999sB%s", "x", "z"]
    yield "precision-debordement-64", ["A%.999999999999999999999999fB%d", "x", "z"]
    yield (
        "classe-imprimable-unicode",
        ["%q|", "a\u00adb", "\u0378", "\u200b", "\ue000", "\U0001f642"],
    )
    yield "guillemet-apres-prefixe", ["%q|", "#'abc", "~'abc", "a#'z", "a~'z"]
    for label, args in [
        ("court", ["%s", "x"]),
        ("texte", ["%s\\n", *(["N" * 48] * 30)]),
        ("binaire", ["%b", *([r"\0\xffA" * 6] * 30)]),
        ("decimal", ["%+.9g\\n", *(["98765.125"] * 30)]),
    ]:
        yield "corpus-performance-" + label, args
    rng = random.Random(27)
    for i, f in enumerate(
        ["%.0f", "%.1f", "%.3e", "%#g", "%#.6g", "%.17g", "%.3a", "%.1g"]
    ):
        values = [
            str(rng.randrange(1, 10**16)) + "e" + str(rng.randrange(-300, 280))
            for _ in range(10)
        ]
        yield (
            f"arrondi-{i}",
            [f + "|", *values, "0.005", "0.995", "0x1.fffffffffffffp-1023"],
        )
    for conv in "diouxXsaAeEfFgGc%bq":
        for flags in ["", "-", "+", " ", "0", "#", "'", "#+0", "-+ ", "'#"]:
            # Bare conversions and modified forms must be checked independently.
            for suffix in (
                ["", "12.4"] if flags or conv not in "%bq" else ["", "9", ".1"]
            ):
                yield (
                    f"conversion-{conv}-{flags}-{suffix}",
                    ["%" + flags + suffix + conv, "27"],
                )
    for length in ["h", "hh", "l", "ll", "L", "j", "t", "z", "hljtz"]:
        yield (
            "longueur-" + length,
            [f"%{length}d %{length}f %{length}s", "400", "0.5", "xyz"],
        )
    for fmt in [
        "%B",
        "%n",
        "%p",
        "%Z",
        "%1$%",
        "%2$*3$.*1$F",
        "%*c",
        "%.0c",
        "%.*c",
        "%#.0o",
        "%#05x",
        "%.0d",
    ]:
        yield "limite-" + fmt, [fmt, "-1", "0", "8"]
    quote = [
        "",
        "abc",
        "a=b",
        "a b",
        "'",
        "a'b",
        "'x\n'",
        "\n",
        "\na\n\tb",
        "a'b$z",
        "é",
        b"\xff",
        "a{}b",
    ]
    quote += [chr(n) for n in range(1, 128)]
    for i in range(0, len(quote), 20):
        yield f"citation-{i}", ["%q|", *quote[i : i + 20]]
    for conv in "fFeEgGaA":
        for prec in ["", ".0", ".1", ".5", ".17", "#.0", "#.9", "+015.5"]:
            yield (
                f"flottants-{conv}-{prec}",
                [
                    "%" + prec + conv + "|",
                    "0",
                    "-0",
                    "-nan",
                    "+inf",
                    "0x1.8p-2",
                    "0.005",
                    "2.5",
                    "3.5",
                    "9.99999",
                    "0.0000999999",
                    "1e-309",
                    "1e999",
                    "0x1p-1074",
                    "0x1.fffffffffffffp1023",
                    "mot",
                    "23.5suffix",
                ],
            )
    for fmt in ["%2$*1$s", "%*3$.*2$s", "%2$s%s", "%3$s%1$s", "%9$b", "%9$q"]:
        yield "indices-" + fmt, [fmt, "4", "2", "6", "abcd", "efgh"]


def result(exe, args, locale, posix=False):
    env = {"LC_ALL": locale, "PATH": os.defpath}
    if posix:
        env["POSIXLY_CORRECT"] = ""
    p = subprocess.run([exe, *args], capture_output=True, env=env, timeout=10)
    return p.returncode, p.stdout, p.stderr


def normalize(err):
    return (
        err.replace(b"Try '/opt/homebrew/bin/gprintf --help'", b"Try 'printf --help'")
        .replace(b"gprintf: ", b"printf: ")
        .replace(b"Try 'gprintf --help'", b"Try 'printf --help'")
    )


def packed(args):
    return [a if isinstance(a, bytes) else a.encode() for a in args]


def triple(t):
    return [t[0], t[1].hex(), t[2].hex()]


def recipe(out):
    out.mkdir(parents=True, exist_ok=True)
    assert b"9.12" in result(GNU, ["--version"], "C")[1].splitlines()[0]
    records = []
    vectors = bytearray()
    for locale_id, locale in enumerate(LOCALES):
        cases = [(k, a, False) for k, a in corpus()]
        cases.append(("constante-posix", ["%d", "'avertissement"], True))
        for name, args, posix in cases:
            argv = packed(args)
            ref = result(GNU, argv, locale, posix)
            actual = result(NOVA, argv, locale, posix)
            expected = ref[0], ref[1], normalize(ref[2])
            doc = name in ("aide", "version")
            if doc:
                path = HERE.parent / "components/printf"
                content = (
                    (path / ("help.fr" if locale_id == 2 else "help.en")).read_bytes()
                    if name == "aide"
                    else b"printf (NovaOS) " + (path / "VERSION").read_bytes()
                )
                expected = (0, content, b"")
            equal = actual == expected
            records.append(
                dict(
                    id=name,
                    locale=locale,
                    posix=posix,
                    argv_hex=[v.hex() for v in argv],
                    gnu=triple(ref),
                    nova=triple(actual),
                    ok=equal,
                )
            )
            # Store exact output for an independent byte comparison, using runs to
            # keep very large field widths out of the read-only segment of the ELF.
            vectors.extend(bytes([locale_id, posix, len(argv), expected[0]]))
            for v in argv:
                vectors.extend(struct.pack("<H", len(v)) + v)
            for v in expected[1:]:
                runs = []
                for c, it in itertools.groupby(v):
                    runs.append((c, sum(1 for _ in it)))
                encoded = b"".join(struct.pack("<BI", c, n) for c, n in runs)
                tag, blob = (1, encoded) if len(encoded) < len(v) else (0, v)
                vectors.extend(struct.pack("<BI", tag, len(blob)) + blob)
    for locale in ["fr_FR.ISO8859-1", "ja_JP.SJIS", "zh_CN.GB18030", "C.UTF-8"]:
        for name, args in [
            ("unicode-charset", [r"\u00E9\u20AC\u4E2D\U000E0001"]),
            ("diagnostic-charset", ["%d", b"x"]),
            ("multioctet-charset", ["%q|", bytes([0x82, 0x5C])]),
        ]:
            a = result(GNU, args, locale)
            b = result(NOVA, args, locale)
            records.append(
                dict(
                    id=name,
                    locale=locale,
                    gnu=triple(a),
                    nova=triple(b),
                    ok=b == (a[0], a[1], normalize(a[2])),
                )
            )
    for ignore in (False, True):

        def broken(exe):
            read, write = os.pipe()
            os.close(read)
            try:

                def setup():
                    signal.signal(
                        signal.SIGPIPE, signal.SIG_IGN if ignore else signal.SIG_DFL
                    )

                p = subprocess.run(
                    [exe, "%20000s", "x"],
                    stdout=write,
                    stderr=subprocess.PIPE,
                    env={"LC_ALL": "C"},
                    preexec_fn=setup,
                )
                return p.returncode, b"", p.stderr
            finally:
                os.close(write)

        a, b = broken(GNU), broken(NOVA)
        records.append(
            dict(
                id="lecteur-ferme-" + str(ignore),
                locale="C",
                gnu=triple(a),
                nova=triple(b),
                ok=b == (a[0], b"", normalize(a[2])),
            )
        )
    for tag in ["fr", "FR", "fr-CA", "fr-zzzz", "fr-zzzzz", "frr", "de-DE"]:
        actual = result(NOVA, ["--help"], tag)
        french = tag in ["fr", "FR", "fr-CA", "fr-zzzz"]
        ref = (
            HERE.parent / "components/printf" / ("help.fr" if french else "help.en")
        ).read_bytes()
        records.append(
            dict(
                id="langue-document",
                locale=tag,
                nova=triple(actual),
                ok=actual == (0, ref, b""),
            )
        )
    # stdout is closed before exec; no filesystem mutation or special device.
    for label, args in [
        ("ferme", ["x"]),
        ("ferme-long", ["%20000s", "x"]),
        ("ferme-vide", [""]),
    ]:

        def closed(exe):
            p = subprocess.run(
                [exe, *args],
                stderr=subprocess.PIPE,
                env={"LC_ALL": "C"},
                preexec_fn=lambda: os.close(1),
            )
            return p.returncode, b"", p.stderr

        a, b = closed(GNU), closed(NOVA)
        records.append(
            dict(
                id=label,
                locale="C",
                gnu=triple(a),
                nova=triple(b),
                ok=b == (a[0], b"", normalize(a[2])),
            )
        )
    (out / "options.json").write_text(
        json.dumps(records, ensure_ascii=False, separators=(",", ":")) + "\n"
    )
    (out / "vecteurs.bin").write_bytes(vectors)
    failed = [r for r in records if not r["ok"]]
    print(f"{len(records)} cas, {len(failed)} écarts")
    for r in failed[:30]:
        print(r["locale"], r["id"], str(r.get("gnu"))[:250], str(r["nova"])[:250])
    return bool(failed)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sortie", type=Path)
    args = parser.parse_args()
    TMP.mkdir(parents=True, exist_ok=True)
    out = args.sortie or Path(tempfile.mkdtemp(prefix="options-", dir=TMP))
    print(out)
    raise SystemExit(recipe(out))
