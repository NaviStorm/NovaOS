"""Comparaison octet par octet avec GNU grep 3.12, dans un bac jetable."""
import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import random
import select
import subprocess
import tempfile
import threading

JOURNAL = b"start\nWarning\nwarning error\nerreur\nok\n\nend\nerreur again\n"
OPTIONS = [
    ("-G --basic-regexp", [r"a\{2\}"], b"a\naa\n"),
    ("-E --extended-regexp", ["warning|error"], JOURNAL),
    ("-F --fixed-strings", ["a.b"], b"a.b\naab\n"),
    ("-P --perl-regexp", [r"(?<=a)b"], b"ab\nac\n"),
    ("-i -y --ignore-case", ["WARNING"], JOURNAL),
    ("--no-ignore-case", ["Warning"], JOURNAL),
    ("-v --invert-match", ["erreur"], JOURNAL),
    ("-w --word-regexp", ["x"], b"xx\nx!\nax\n"),
    ("-x --line-regexp", ["erreur"], JOURNAL),
    ("-e --regexp", ["erreur", "-e", "start"], JOURNAL),
    ("-f --file", ["motifs", "journal.txt"], b""),
    ("-n --line-number", ["erreur", "journal.txt"], b""),
    ("-c --count", ["erreur"], JOURNAL),
    ("-l --files-with-matches", ["erreur", "autre", "journal.txt"], b""),
    ("-L --files-without-match", ["erreur", "autre", "journal.txt"], b""),
    ("-o --only-matching", ["ab*"], b"ab abbb\na\n"),
    ("-q --quiet --silent", ["erreur", "absent", "journal.txt"], b""),
    ("-s --no-messages", ["x", "absent"], b""),
    ("-A --after-context", ["1", "erreur"], JOURNAL),
    ("-B --before-context", ["2", "erreur"], JOURNAL),
    ("-C --context", ["1", "erreur"], JOURNAL),
    ("-1", ["erreur"], JOURNAL),
    ("--group-separator=SEP", ["-C0", "erreur"], JOURNAL),
    ("--no-group-separator", ["-C0", "erreur"], JOURNAL),
    ("-r --recursive", ["TODO", "sources/"], b""),
    ("-R --dereference-recursive", ["TODO", "sources/"], b""),
    ("--include=*.rs", ["-r", "TODO", "sources/"], b""),
    ("--exclude=*.rs", ["-r", "TODO", "sources/"], b""),
    ("--exclude-dir=ignore", ["-r", "TODO", "sources/"], b""),
    ("--exclude-from=exclusions", ["-r", "TODO", "sources/"], b""),
    ("-d --directories", ["skip", "TODO", "sources/"], b""),
    ("-D --devices", ["skip", "TODO", "fifo"], b""),
    ("-H --with-filename", ["erreur", "journal.txt"], b""),
    ("-h --no-filename", ["erreur", "journal.txt", "journal.txt"], b""),
    ("--label=flux", ["-H", "erreur"], JOURNAL),
    ("--color=always --colour=always", ["erreur"], JOURNAL),
    ("-a --text --binary-files=text", ["x"], b"ax\0b\n"),
    ("-I --binary-files=without-match", ["x"], b"ax\0b\n"),
    ("--binary-files=binary", ["x"], b"ax\0b\n"),
    ("-z --null-data", ["abc"], b"1\nabc\0b\0"),
    ("-Z --null", ["-l", "erreur", "journal.txt"], b""),
    ("-b --byte-offset", ["-o", "erreur"], JOURNAL),
    ("-T --initial-tab", ["-Hn", "erreur", "journal.txt"], b""),
    ("-m --max-count", ["1", "erreur"], JOURNAL),
    ("--line-buffered", ["erreur"], JOURNAL),
    ("-U --binary", ["x"], b"x\r\n"),
    ("--", ["-n"], b"-n\nn\n"),
    ("--help", [], b""),
    ("-V --version", [], b""),
]


def cases(seed, fuzz):
    for forms, args, data in OPTIONS:
        for form in forms.split():
            yield form, [form, *args], data
    for name, args, data in [
        ("stdin-cat", ["erreur"], JOURNAL),
        ("combiné", ["-Ei", "warning|error", "journal.txt"], b""),
        ("absent-motif", ["introuvable", "journal.txt"], b""),
        ("absent-fichier", ["x", "absent"], b""),
        ("permission", ["x", "interdit"], b""),
        ("permission-arbre", ["-r", "x", "ferme"], b""),
        ("option-inconnue", ["--inconnue"], b""),
        ("regex-invalide", ["["], b""),
        ("pcresyntax", ["-P", "("], b""),
        ("pcre-ordre", ["-Po", "a|ab"], b"ab\n"),
        ("pcre-digits", ["-Po", r"\d"], "٣ 3\n".encode()),
        ("pcre-vide", ["-Po", "x*"], "été\n".encode()),
        ("pcre-echec-limite", ["-P", "(*LIMIT_MATCH=1)(a+)+$"], b"a"*64+b"b\n"),
        ("grand-fichier-motifs", ["-Fcf", "motifs-longs"], b"xxxxxxxxx\nfin\n"),
        ("multimotif", ["-e", "erreur", "-f", "motifs"], JOURNAL),
        ("alternance", ["-Eo", "a|ab"], b"ab\n"),
        ("priorité", ["-i", "--no-ignore-case", "Warning"], JOURNAL),
        ("fin-sans-lf", ["x"], b"x"),
        ("octets", ["."], b"\xff\n"),
        ("binaire-tardif", ["x"], b"x\ny\0\n"),
        ("UTF-8", ["-i", "été"], "ÉTÉ\nété\n".encode()),
        ("ligne-longue", ["-c", "fin"], b"x"*1048576+b"fin\n"),
        ("utf8-dot-bloque", ["-a", "x.*y"], b"x\xffy\n"),
        ("utf8-ancre-debut", ["-a", "^y"], b"x\xffy\n"),
        ("utf8-ancre-fin", ["-a", "x$"], b"x\xffy\n"),
        ("utf8-derniere-ancre", ["-a", "$"], b"\xff\n"),
        ("utf8-position", ["-abo", "y"], b"x\xffy\n"),
        ("binaire-comptage", ["-c", "x"], b"x\x00x\n"),
        ("binaire-ancre", ["^x$"], b"x\x00x\n"),
        ("binaire-ignore-compte", ["-Ic", "x"], b"x\x00x\n"),
        ("texte-avant-ignore", ["-aI", "x"], b"x\0y\n"),
        ("ignore-avant-texte", ["-Ia", "x"], b"x\0y\n"),
        ("abreviation", ["--files-with-mat", "x"], b"x\n"),
        ("limite-infinie", ["-m", "-1", "erreur"], JOURNAL),
        ("limite-invalide", ["-m", "xyz", "a"], JOURNAL),
        ("limite-zero", ["-m0", "x", "absent"], b""),
        ("limite-memoire", ["-c", "Z"], b"a"*(8*1024*1024+10)+b"Z\n"),
        ("pcre-repete", ["-P", "-e", "a", "-e", "a"], b"a\n"),
        ("pcre-utf8-look", ["-Pbo", "(?<=é)t"], "été\n".encode()),
        ("plusieurs-motifs", ["-Eo", "-e", "ab", "-e", "a"], b"ababa\n"),
        ("mot-plus-court", ["-Ewo", "a-|a"], b"a-b\n"),
        ("chevauchement", ["-wo", "@x@"], b"a@x@x@!\n"),
        ("mot-entier-pcre", ["-Pw", "a|ab"], b"ab\n"),
        ("fichier-exclu", ["--exclude=*.txt","erreur","journal.txt"], b""),
        ("regex-classes", ["-Eo", "[[:alpha:]]+"], "123 été\n".encode()),
        ("regex-reference", ["-G", r"^\(ab\)\1$"], b"abab\naba\n"),
        ("motif-fichier-nonutf8", ["-Faf","motif-brut"], b"b\xffc\n"),
        ("limite-regex", ["-Ec", "x.*Z"], b"x"*(8*1024*1024+10)+b"Z\n"),
        ("limite-contexte", ["-B", "5000", "zzz"], (b"x"*4095+b"\n")*4200),
        ("frontiere-64k", ["-Fc", "XYZ"], b"a"*65535+b"XYZ\nXYZ"),
        ("frontiere-64k-inverse", ["-Fvc", "XYZ"], b"a"*65535+b"XYZ\nz"),
        ("fin-fichier-vide", ["-Fvc", "XYZ"], b""),
        ("reprise-contexte", ["-A2","-m1","x"], b"x\nx\ny\nz\n"),
        ("stdout-couleur-auto", ["--color=auto","x"], b"x\n"),
        ("stdout-couleur-never", ["--color=never","x"], b"x\n"),
        ("dossier-lire", ["-d","read","x","sources"], b""),
        ("dossier-recursif", ["-d","recurse","TODO","sources/"], b""),
        ("fichier-tiret", ["-e","erreur","-"], JOURNAL),
        ("patterns-stdin", ["-f","-","journal.txt"], b"erreur\nstart\n"),
        ("quiet-erreur-apres", ["-q","erreur","journal.txt","absent"], b""),
        ("nonutf8-fragments", ["-Faof","motif-brut"], b"b\xffc\n"),
        ("pcre-fin-z", ["-zPo","a$"], b"a\n\0"),
        ("sans-motif", [], b""),
        ("invalide-C", ["-C","abc","x"], b""),
        ("invalide-D", ["-D","bad","x"], b""),
        ("invalide-d", ["-d","bad","x"], b""),
        ("invalide-binaire", ["--binary-files=bad","x"], b""),
        ("invalide-couleur", ["--color=foo","x"], b""),
        ("motifs-vides", ["-f", "vide"], JOURNAL),
        ("option-courte-inconnue", ["-J"], b""),
        ("fifo-read", ["-D", "read", "x"], b"x\n"),
    ]:
        yield name, args, data
    rng = random.Random(seed)
    for i in range(fuzz):
        lines = [bytes(rng.choice(b"aAbc_1. !") for _ in range(rng.randrange(30)))
                 for _ in range(rng.randrange(12))]
        data = b"\n".join(lines)
        mode = rng.choice(["-G", "-E", "-F", "-P"])
        flags = rng.sample(["-n", "-x", "-w", "-v", "-o", "-c", "-i"], rng.randrange(4))
        pattern = rng.choice(["", "a", "a|ab", ".", "^", "$", "[ab]", "a*", "a+", "(a)", r"\b", "[[:digit:]]"])
        yield f"fuzz-{i}", [mode, *flags, "-e", pattern], data

    for i in range(fuzz // 4):
        chunks = [rng.choice([b"x", b" ", bytes([0]), bytes([10]), bytes([255]), "é".encode(), b"a", b"z"])
                  for _ in range(rng.randrange(1, 72))]
        data = b"".join(chunks)
        mode = rng.choice(["-F", "-P", "-G", "-E"])
        pattern = rng.choice(["x", "é", ".", "[a-z]", "^", "$", "a.*z", "a|x", ""])
        flags = rng.sample(["-a", "-I", "-z", "-i", "-o", "-n", "-c", "-v"], rng.randrange(4))
        yield f"octets-{i}", [mode, *flags, "-e", pattern], data


def run(binpath, args, data, cwd, env):
    p = subprocess.run(["grep", *args], executable=binpath, input=data, capture_output=True, cwd=cwd, env=env, timeout=10)
    return [p.returncode, p.stdout.hex(), p.stderr.hex()]


def pipe_test(binpath, mode, cwd, env):
    if mode == "fifo":
        with subprocess.Popen([binpath, "-D", "read", "TODO", "fifo"], cwd=cwd, env=env,
                              stdout=subprocess.PIPE, stderr=subprocess.PIPE) as p:
            def feed():
                with open(Path(cwd)/"fifo","wb",buffering=0) as stream:
                    stream.write(b"before\nTODO\n")
            thread = threading.Thread(target=feed, daemon=True)
            thread.start()
            out, err = p.communicate(timeout=5)
            thread.join(timeout=1)
            assert not thread.is_alive()
            return [p.returncode,out.hex(),err.hex()]
    if mode == "reprise-m1":
        with open(Path(cwd)/"journal.txt","rb",buffering=0) as stream:
            p = subprocess.run([binpath,"-m1","-A1","erreur"],stdin=stream,cwd=cwd,env=env,capture_output=True)
            return [p.returncode,p.stdout.hex(),p.stderr.hex(),stream.read().hex()]
    if mode == "fermeture":
        read_fd, write_fd = os.pipe()
        os.close(read_fd)
        try:
            with subprocess.Popen([binpath, "x"], stdin=subprocess.PIPE, stdout=write_fd,
                                  stderr=subprocess.PIPE, cwd=cwd, env=env) as p:
                _, err = p.communicate(b"x\n"*100000, timeout=5)
                return [p.returncode, "", err.hex()]
        finally:
            os.close(write_fd)
    with subprocess.Popen([binpath, "--line-buffered", "x"], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=cwd, env=env) as p:
        p.stdin.write(b"x\n")
        p.stdin.flush()
        ready = bool(select.select([p.stdout], [], [], 1)[0])
        out, err = p.communicate(timeout=5)
        return [p.returncode, out.hex(), err.hex(), ready]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("candidate", type=Path)
    ap.add_argument("report", type=Path)
    ap.add_argument("--oracle", default="/opt/homebrew/opt/grep/libexec/gnubin/grep")
    ap.add_argument("--locale", default="C")
    ap.add_argument("--seed", type=int, default=270312)
    ap.add_argument("--fuzz", type=int, default=300)
    a = ap.parse_args()
    bins = [a.oracle, str(a.candidate.resolve())]
    env = dict(PATH=os.environ["PATH"], LC_ALL=a.locale, LANG=a.locale, TERM="dumb",
               TMPDIR=os.environ["TMPDIR"])
    info = subprocess.check_output([bins[0], "--version"], env=env).decode()
    assert info.splitlines()[0] == "grep (GNU grep) 3.12"
    probe = run(bins[0], ["-Po", r"(?<=a)b"], b"ab\n", ".", env)
    assert probe == [0, b"b\n".hex(), ""]
    rows = []
    with tempfile.TemporaryDirectory(dir=env["TMPDIR"], prefix="grep-campagne-") as d:
        root = Path(d)
        for name, data in {"journal.txt": JOURNAL, "autre": b"ok\n", "motifs": b"Warning\nok\n",
                           "exclusions": b"*.rs\n", "vide": b"", "motif-brut": b"\xff\n", "sources/a.rs": b"TODO\n",
                           "sources/ignore/b": b"TODO\n", "interdit": b"x\n",
                           "ferme/x": b"x\n"}.items():
            f = root / name
            f.parent.mkdir(exist_ok=True, parents=True)
            f.write_bytes(data)
        with (root / "motifs-longs").open("wb") as stream:
            for _ in range(144):
                stream.write((b"x"*63 + b"\n")*1024)
            stream.write(b"fin")
        (root / "sources/lien").symlink_to("a.rs")
        os.mkfifo(root / "fifo")
        (root/"interdit").chmod(0)
        (root/"ferme").chmod(0)
        try:
            for name, args, data in cases(a.seed, a.fuzz):
                results = [run(b, args, data, d, env) for b in bins]
                rows.append(dict(test=name, argv=args, entree_sha256=hashlib.sha256(data).hexdigest(), oracle=results[0], candidat=results[1],
                                 identique=results[0] == results[1]))
            for mode in ("fermeture", "vidage-avant-EOF", "fifo", "reprise-m1"):
                x, y = [pipe_test(b, mode, d, env) for b in bins]
                rows.append(dict(test=mode, oracle=x, candidat=y, identique=x == y))
        finally:
            (root/"ferme").chmod(0o700)
    a.report.write_text(json.dumps(dict(oracle=info, pcre=probe, locale=a.locale,
        graine=a.seed, fuzz=a.fuzz, mesures=rows), ensure_ascii=False, indent=1)+"\n")
    counts = Counter(r["identique"] for r in rows)
    print(a.locale, dict(counts))
    return int(counts[False] > 0)


if __name__ == "__main__":
    raise SystemExit(main())
