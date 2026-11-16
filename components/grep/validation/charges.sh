#!/bin/sh
# ARC-027 — banc de charge Darwin ARM64, données synthétiques et oracle GNU.
set -eu
case "$#" in
    0) set -- 10000000 65000000 ;;
    1) case "$1" in
        -h|--help) echo "Usage : ./charges.sh [NOMBRE_DE_LIGNES ...]"
            echo "Par défaut : 10 000 000 puis 65 000 000, deux passages par moteur et motif."
            exit 0 ;;
    esac ;;
esac
livraison=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
: "${NOVAOS_TMPDIR:=$(dirname -- "$livraison")/tmp}"
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/"
export LC_ALL=en_US.UTF-8 LANG=en_US.UTF-8
unset GREP_OPTIONS GREP_COLOR GREP_COLORS POSIXLY_CORRECT
exec python3 -B - "$livraison" "$@" <<'PY'
import hashlib
import json
import locale
import os
from pathlib import Path
import platform
import random
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import time

delivery = Path(sys.argv[1])
if platform.system() != "Darwin" or platform.machine() != "arm64":
    sys.exit("Le banc mesure le RSS Darwin ARM64 en octets.")
if not all(s.isascii() and s.isdecimal() and 1 <= int(s) <= 99_999_999 for s in sys.argv[2:]):
    sys.exit("Choisir des tailles entières comprises entre 1 et 99 999 999.")
sizes = list(map(int, sys.argv[2:]))
if len(set(sizes)) != len(sizes):
    sys.exit("Choisir des tailles distinctes.")
locale.setlocale(locale.LC_ALL, "")
executables = {"livré": delivery / "grep", "GNU": Path("/opt/homebrew/bin/ggrep")}
def digest(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f, "sha256").hexdigest()

metadata = {}
for name, exe in executables.items():
    version = subprocess.check_output([exe, "--version"], text=True).strip()
    if name == "GNU" and version.splitlines()[0] != "ggrep (GNU grep) 3.12":
        sys.exit("GNU grep 3.12 est requis.")
    probe = subprocess.run([exe, "-Po", "(?<=a)b"], input=b"ab\n", capture_output=True)
    if (probe.returncode, probe.stdout, probe.stderr) != (0, b"b\n", b""):
        sys.exit(f"Échec du témoin PCRE2 pour {name}.")
    metadata[name] = dict(path=str(exe), sha256=digest(exe), version=version, pcre=True)

cases = [
    ("ere-journal", "-Ec",
     r"^id=[0-9]{8} (WARN|ERROR) (api|worker)/(fr|be) /(orders|users)/[0-9]{4} (429|5[0-9]{2}) [1-9][0-9]{3}ms [[:alnum:]]{3}:[[:alnum:]]{3} (retry|timeout)$"),
    ("bre-rappel", "-Gc", r" \([a-f]\{2\}[0-9]\):\1 \(retry\|timeout\|refusé\)$"),
    ("pcre-extraction", "-nPo",
     r"^(?=id=\d{8} ERROR (?:api|batch)/)(?=.* (?:429|5\d{2}) )(?=.* (?<tag>[a-f]{2}\d):\k<tag> timeout$)id=\K\d{8}(?= ERROR )"),
    ("pcre-absence", "-Pc",
     r"^id=\d{8} (?:INFO|WARN|ERROR) (?:api|batch|worker)/(?:fr|be|ca) /(?:users|orders|jobs)/\d{4} [245]\d{2} \d{1,4}ms (?<tag>[a-f]{2}\d):\k<tag> ok$"),
]
# Le modèle des résultats est établi sur les champs, indépendamment des regex.
rng = random.Random(270312)
templates = []
for _ in range(65_536):
    level = rng.choice(("INFO", "WARN", "ERROR"))
    service = rng.choice(("api", "worker", "batch"))
    region = rng.choice(("fr", "be", "ca"))
    route = rng.choice(("orders", "users", "jobs"))
    key = rng.randrange(10000)
    status = rng.choice((200, 204, 400, 404, 429, 500, 503, 507))
    ms = rng.randrange(1, 10000)
    action = rng.choice(("ok", "retry", "timeout", "refusé"))
    a, b, digit = rng.choice("abcdef"), rng.choice("abcdef"), rng.randrange(10)
    same = rng.randrange(6) == 0 and action != "ok"
    pair = f"{a}{b}{digit}:{a}{b}{digit if same else (digit+1)%10}"
    template = f" {level} {service}/{region} /{route}/{key:04d} {status} {ms}ms {pair} {action}\n".encode()
    error = status == 429 or 500 <= status < 600
    selected = (level in ("WARN", "ERROR") and service in ("api", "worker") and
                region in ("fr", "be") and route in ("orders", "users") and
                error and ms >= 1000 and action in ("retry", "timeout"))
    extract = level == "ERROR" and service in ("api", "batch") and error and same and action == "timeout"
    templates.append((template, selected, same and action in ("retry", "timeout", "refusé"), extract))
bodies = [row[0] for row in templates]
indices = [[i for i, row in enumerate(templates) if row[k]] for k in (1, 2, 3)]
period = len(bodies)

def generate(path, size):
    sha = hashlib.sha256()
    extracted = hashlib.sha256()
    counts = [0, 0, 0, 0]
    extract_bytes = 0
    with path.open("xb") as f:
        for start in range(0, size, period):
            n = min(size - start, period)
            block = b"".join(b"id=%08d" % (start+i+1) + bodies[i] for i in range(n))
            assert block.count(b"\n") == n
            f.write(block)
            sha.update(block)
            for k, offsets in enumerate(indices):
                counts[k] += len(offsets) if n == period else sum(i < n for i in offsets)
            output = b"".join(b"%d:%08d\n" % (start+i+1, start+i+1) for i in indices[2] if i < n)
            extracted.update(output)
            extract_bytes += len(output)
    expected = {}
    for (name, _, _), count in zip(cases, counts, strict=True):
        out = f"{count}\n".encode()
        extraction = name == "pcre-extraction"
        expected[name] = dict(selections=count, statut=0 if count else 1,
                              octets=extract_bytes if extraction else len(out),
                              sha256=extracted.hexdigest() if extraction else hashlib.sha256(out).hexdigest())
    return dict(lignes=size, octets=path.stat().st_size, sha256=sha.hexdigest(), attendus=expected)

def measure(exe, args, corpus, out, err, stdin):
    with corpus.open("rb") as inp, out.open("wb") as dest, err.open("wb") as diag:
        argv = [str(exe), *args] + ([] if stdin else [str(corpus)])
        t0 = time.monotonic()
        proc = subprocess.Popen(argv, stdin=inp if stdin else subprocess.DEVNULL, stdout=dest, stderr=diag)
        _, wait_status, usage = os.wait4(proc.pid, 0)
        seconds = time.monotonic() - t0
        proc.returncode = os.waitstatus_to_exitcode(wait_status)
    return dict(statut=proc.returncode, secondes=seconds, rss_octets=usage.ru_maxrss,
                cpu_user_s=usage.ru_utime, cpu_systeme_s=usage.ru_stime,
                octets=out.stat().st_size, sha256=digest(out),
                stderr_octets=err.stat().st_size, stderr_sha256=digest(err),
                commande=shlex.join(argv) + (" < "+shlex.quote(str(corpus)) if stdin else ""))

# Une ligne par traitement, dans le terminal et le rapport Markdown.
traitements = {
    "ere-journal": "Filtrage du journal (ERE)",
    "bre-rappel": "Référence arrière (BRE)",
    "pcre-extraction": "Extraction des identifiants (PCRE)",
    "pcre-absence": "Recherche sans résultat (PCRE)",
}
colonnes = ("Lignes", "Traitement", "Nova grep", "GNU grep")
table_row = "{:<11}  {:<34}  {:>10}  {:>10}"
headings = table_row.format(*colonnes)

def resumer_cas(size, name, selected, rows, terminal_style):
    millions, reste = divmod(size, 1_000_000)
    if millions and reste == 0:
        volume = f"{millions} million" + ("s" if millions > 1 else "")
    else:
        volume = f"{size:,}".replace(",", " ")
    cells = []
    details = []
    valid = True
    for label in ("livré", "GNU"):
        runs = [r for r in rows if r["moteur"] == label]
        median = statistics.median(r["secondes"] for r in runs)
        peak = max(r["rss_octets"] for r in runs) / 1024**2
        correct = all(r["conforme"] for r in runs)
        cells.append(f"{median:.3f} s".replace(".", ",") if correct else "ÉCHEC")
        valid = valid and correct
        details.append((median, peak))
    terminal = [v.rjust(10) for v in cells]
    markdown = cells.copy()
    if valid and cells[0] != cells[1]:
        best = 0 if details[0][0] < details[1][0] else 1
        markdown[best] = f"**{cells[best]}**"
        if terminal_style:
            terminal[best] = f"\033[1m{terminal[best]}\033[0m"
    print(table_row.format(volume, traitements[name], *terminal), flush=True)
    summary = "| " + " | ".join((volume, traitements[name], *markdown)) + " |"
    count = f"{selected:,}".replace(",", " ")
    peaks = [f"{peak:.2f}".replace(".", ",") for _, peak in details]
    memory = f"| {volume} | {traitements[name]} | {count} | {peaks[0]} | {peaks[1]} |"
    return summary, memory

scratch = Path(os.environ["NOVAOS_TMPDIR"])
scratch.mkdir(exist_ok=True, parents=True)
proof = Path(tempfile.mkdtemp(prefix="charges-", dir=delivery))
terminal_style = sys.stdout.isatty() and os.environ.get("TERM") != "dumb" and not os.environ.get("NO_COLOR")
print(headings)
print("─" * len(headings), flush=True)
summary_lines = []
memory_lines = []
report = dict(date=time.strftime("%Y-%m-%dT%H:%M:%S%z"), macos=platform.mac_ver()[0],
              arch=platform.machine(), locale=os.environ["LC_ALL"], graine=270312, profils=period,
              executable=metadata, script_sha256=digest(delivery/"charges.sh"), repetitions=2,
              tailles=sizes, cas=cases, corpus=[], mesures=[])
def save():
    tmp = proof/"mesures.part"
    tmp.write_text(json.dumps(report, ensure_ascii=False, indent=2)+"\n")
    tmp.replace(proof/"mesures.json")
save()
with tempfile.TemporaryDirectory(prefix="arc027-charge-", dir=scratch) as tmp:
    root = Path(tmp)
    for size in sizes:
        if shutil.disk_usage(root).free < size * (11+max(map(len,bodies))) + 512*1024**2:
            sys.exit("Espace disque insuffisant pour ce volume.")
        corpus = root/"journal"
        data = generate(corpus, size)
        # Lecture intégrale pour vérifier le fichier réellement fourni aux processus.
        assert digest(corpus) == data["sha256"]
        report["corpus"].append(data)
        save()
        if size != sizes[0]:
            print("─" * len(headings), flush=True)
        for name, flags, pat in cases:
            case_rows = []
            for rep, order in enumerate((("livré", "GNU"), ("GNU", "livré")), 1):
                for label in order:
                    out, err = root/"stdout", root/"stderr"
                    row = measure(executables[label], [flags, pat], corpus, out, err, name=="pcre-extraction")
                    expected = data["attendus"][name]
                    row["conforme"] = (all(row[k] == expected[k] for k in ("statut","octets","sha256"))
                                       and row["stderr_octets"] == 0)
                    row.update(lignes=size, cas=name, moteur=label, repetition=rep)
                    report["mesures"].append(row)
                    save()
                    case_rows.append(row)
                    if not row["conforme"]:
                        print(f"Échec : {name}, {label}, passage {rep}", file=sys.stderr, flush=True)
                        tag = f"{size}-{name}-{label}-{rep}"
                        shutil.copyfile(out, proof/(tag+".stdout"))
                        shutil.copyfile(err, proof/(tag+".stderr"))
            selected = data["attendus"][name]["selections"]
            summary, memory = resumer_cas(size, name, selected, case_rows, terminal_style)
            summary_lines.append(summary)
            memory_lines.append(memory)
        corpus.unlink()
correct = sum(r["conforme"] for r in report["mesures"])
lines = ["# Charge grep sur macOS ARM64", "",
         f"{correct}/{len(report['mesures'])} exécutions conformes au modèle indépendant.",
         "Temps médian de deux passages par binaire, RSS maximal observé. Cache système ordinaire, ordre alterné.",
         "Les lignes ont un identifiant unique et un corps synthétique de période 65 536. Le motif d’extraction lit stdin sur un fichier régulier.", "",
         "| " + " | ".join(colonnes) + " |", "|---|---|---:|---:|", *summary_lines,
         "", "## Sélections attendues et mémoire résidente maximale", "",
         "| Lignes | Traitement | Sélections | Nova grep (Mio) | GNU grep (Mio) |",
         "|---|---|---:|---:|---:|", *memory_lines]
(proof/"rapport.md").write_text("\n".join(lines)+"\n")
shutil.copyfile(delivery/"charges.sh", proof/"charges.sh")
fichiers = sorted(p for p in proof.iterdir() if p.is_file())
(proof/"SHA256SUMS").write_text("".join(f"{digest(p)}  {p.name}\n" for p in fichiers))
print(f"\nBilan : {correct}/{len(report['mesures'])} exécutions conformes.")
print(f"Rapport : {proof / 'rapport.md'}", flush=True)
sys.exit(0 if correct==len(report["mesures"]) else 1)
PY
