"""Contrat de nsh et des délégations de retrait, sur des volumes P9 jetables."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

from cli import HOST, ROOT, Terminal


def fixture(work, sector):
    directories = ["g", "g/src", "g/src/lib", "g/src/lib/profond", "g/.cache",
                   "sortie", "archive", "colonnes", "limite", "probe-arbre", "probe-vide", "coupure"]
    files = {
        "g/1.txt": "un", "g/A.txt": "majuscule", "g/a.txt": "minuscule",
        "g/-nom": "tiret", "g/.cache/secret.txt": "cache", "g/é.txt": "accent",
        "g/src/test1.rs": "direct", "g/src/lib/test2.rs": "imbriqué",
        "g/src/lib/profond/test3.rs": "profond", "g/src/lib/b.txt": "feuille",
        "g/[A].lit": "crochets", "g/lit*.rs": "étoile", "g/[abc": "inachevé",
        "g/界界界界界界界界界": "large", "temoin": "intact",
        "colonnes/a": "a", "colonnes/b": "douze octets", "probe-arbre/alpha": "protégé", "coupure/alpha": "premier", "coupure/beta": "second",
    }
    script = ["mkdir /" + d for d in directories]
    for name, contents in files.items():
        script.extend(["create /" + name, f"write /{name} 0 {contents.encode().hex()}"])
    script.extend(f"create /limite/e{i:02d}" for i in range(34))
    raw, image = work / "origine.raw", work / "p9.raw"
    with raw.open("xb") as output:
        meta = subprocess.run([HOST, "script", "512", str(sector)], check=True,
                              input=("\n".join(script)+"\n").encode(), stdout=output, stderr=subprocess.PIPE)
    fs, lv = re.search(rb"FS=(\w+) LV=(\w+)", meta.stderr).groups()
    with raw.open("rb") as source, image.open("xb") as output:
        subprocess.run([HOST, "migrate-content-p9", "512", str(sector), fs, lv],
                       stdin=source, stdout=output, check=True)
    return image, fs, lv


def gnu_transferts(work):
    """Oracles GNU pour les opérations où le contrat de publication coïncide."""
    host = work / "gnu"
    host.mkdir()
    for path in ("source", "sortie", "archive"):
        (host / path).mkdir()
    for path, value in [("1.txt", "un"), ("é.txt", "accent"), ("source/feuille", "profond")]:
        (host / path).write_text(value)
    def cmd(*args):
        return subprocess.check_output(args, cwd=host)
    cmd("gcp", "--", "1.txt", "é.txt", "sortie")
    cmd("gmv", "--", "sortie/1.txt", "sortie/é.txt", "archive")
    contenu = cmd("gcat", "archive/1.txt", "archive/é.txt").decode()
    cmd("gcp", "-R", "source", "sortie")
    cmd("gmv", "sortie/source", "archive")
    feuille = cmd("gcat", "archive/source/feuille").decode()
    assert subprocess.run(["grmdir", "archive"], cwd=host, stderr=subprocess.DEVNULL).returncode != 0
    cmd("grmdir", "sortie")
    cmd("grm", "-r", "archive")
    assert (host/"source/feuille").is_file() and not (host/"archive").exists()
    return contenu, feuille


def check(arch, sector, work):
    image, fs, lv = fixture(work, sector)
    contenu, feuille = gnu_transferts(work)
    checked = []
    with Terminal(arch, sector, image, "nsh", test=True) as t:
        t.until(b"nsh> ")

        def run(command, code=0, contains=(), absent=(), answers=()):
            t.vm.stdin.write(command.encode()+b"\n")
            t.until(b"\r\n")
            for answer in answers:
                t.until(b"[o/N] ")
                t.vm.stdin.write(answer.encode()+b"\n")
            serial = t.until(b"nsh> ").decode()
            out = re.sub(r"BLOC parcours i=\d+ octets=\d+ ns=\d+\r\n", "", serial)
            expected = list(contains)
            if code is not None:
                expected.append(f"[statut sortie {code}]")
            for marker in expected:
                assert marker in out, (command, marker, out)
            for marker in absent:
                assert marker not in out, (command, marker, out)
            checked.append(command)
            return out

        run("cd g/src/lib", None)
        run("pwd", contains=["/g/src/lib\r\n"])
        run("cd .././lib/profond/", None)
        run("pwd", contains=["/g/src/lib/profond\r\n"])
        run("cat ../test2.rs", contains=["imbriqué"])
        run("cd ../../..", None)
        run("pwd", contains=["/g\r\n"])
        run("cd inexistent/..", None, ["introuvable"])
        run("cd a.txt/..", None, ["répertoire"])
        run("cd ../..", None, ["accès refusé"])
        run("pwd", contains=["/g\r\n"])
        run("ls src/", contains=["test1.rs", "lib"])
        run("cat src//test1.rs", 3)
        run("cat -- -nom", contains=["tiret"])
        run("cat -n*", contains=["tiret"])
        run("cat '[A].lit' lit\\*.rs '[abc'", contains=["crochets", "étoile", "inachevé"])
        run("cat '*.txt'", 20)
        run("cat \"**\"/*.txt", 20)
        run("cat introuvable*.rs", 20)
        run("cat [z-a].txt", None, ["argument invalide"], ["CLI spawn"])
        for pattern, expected in [
            ("[0-9].txt", ["1.txt"]), ("[a-z].txt", ["a.txt"]),
            ("[A-Z].txt", ["A.txt"]), ("[!1aé].txt", ["A.txt"]),
            ("[aA].txt", ["A.txt", "a.txt"]), ("?.txt", ["1.txt", "A.txt", "a.txt", "é.txt"]),
            ("*.txt", ["1.txt", "A.txt", "a.txt", "é.txt"]),
            ("**/*.txt", ["1.txt", "A.txt", "a.txt", "src/lib/b.txt", "é.txt"]),
            ("src/**/test?.rs", ["src/lib/profond/test3.rs", "src/lib/test2.rs", "src/test1.rs"]),
            ("src/*/test?.rs", ["src/lib/test2.rs"]), (".cache/**/*.txt", [".cache/secret.txt"]),
        ]:
            out = run("ls -d -- "+pattern, contains=["\r\n".join(expected)+"\r\n"])
            assert "secret.txt" not in out or pattern.startswith(".cache/")
        visible = ["./-nom","1.txt","A.txt","[A].lit","[abc","a.txt","lit*.rs","src","src/lib",
                   "src/lib/b.txt","src/lib/profond","src/lib/profond/test3.rs","src/lib/test2.rs",
                   "src/test1.rs","é.txt","界界界界界界界界界"]
        for pattern in ("**", "**/*", "**/**"):
            out = run("ls -d -- "+pattern, contains=["\r\n".join(visible)+"\r\n"], absent=["secret.txt",".cache"])
            assert out.count("src/lib/test2.rs\r\n") == 1
        run("ls -d -- src/lib/profond/**", contains=["src/lib/profond\r\nsrc/lib/profond/test3.rs\r\n"])
        out = run("wc -c **/*.txt")
        names = re.findall(r"^\d+ ([^\r\n]+)\r?$", out, re.M)
        assert names == ["1.txt", "A.txt", "a.txt", "src/lib/b.txt", "é.txt", "total"], names
        run("ls -d -- */", contains=["src/\r\n"], absent=["src/lib"])
        run("ls ../limite/*", None, ["limite atteinte"], ["CLI spawn"])
        run("ls -ldw8 -- 界界界界界界界界界", contains=["界界界界\r\n界界界界\r\n界\r\n"])
        run("ls -l -Z", 3, ["non prise en charge : -Z"])
        run("cp --preserve=all a.txt ../sortie", 3)
        run("mv -r a.txt ../sortie", 3)
        run("rm --one-file-system src", 3)
        run("rmdir --parents src", 3)
        run("cat --number a.txt", 3)
        run("head -z a.txt", 3)
        run("tail --follow a.txt", 3)
        run("wc --files0-from=a.txt", 3)
        # Une demande d’information n’accorde aucun droit, quel que soit le nombre de noms.
        for name, operands in (("rm", "1.txt A.txt"), ("rmdir", "../archive ../sortie"),
                               ("cp", "1.txt A.txt ../sortie"), ("mv", "1.txt A.txt ../sortie")):
            run(f"{name} --help {operands}", contains=[name + " ["])
            run(f"{name} --version {operands}", contains=["NovaOS 0.16.3"])
        run("cp --help 1.txt A.txt ../absent", contains=["cp ["])
        run("mv --version 1.txt A.txt ../absent", contains=["NovaOS 0.16.3"])
        run("cat 1.txt A.txt", contains=["unmajuscule"])
        run("ls -d ../archive ../sortie", contains=["../archive", "../sortie"])
        run("ls ../sortie", absent=["1.txt", "A.txt"])
        run("cp *.txt ../sortie/")
        run("cat ../sortie/1.txt ../sortie/é.txt", contains=[contenu])
        run("cp -f A.txt ../sortie", 27)
        run("cp --interactive a.txt ../sortie", 27, answers=["o"])
        run("mv --force A.txt ../sortie", 27)
        run("mv -i a.txt ../sortie", 27, answers=["o"])
        run("mv -n A.txt ../sortie")
        run("cat A.txt a.txt", contains=["majusculeminuscule"])
        run("mv ../sortie/1.txt ../sortie/é.txt ../archive")
        run("cat ../archive/1.txt ../archive/é.txt", contains=[contenu])
        run("cp -R src ../sortie")
        run("mv ../sortie/src ../archive")
        run("cat ../archive/src/lib/profond/test3.rs", contains=[feuille])
        run("cp A.txt a.txt 1.txt", None, ["répertoire"], ["CLI spawn"])
        run("cd /", None)
        for scope in ["arbre","vide","ancien-retrait","arbre-revoque","vide-revoque"]:
            run("@probe "+scope)
        run("cat temoin probe-arbre/alpha", contains=["intactprotégé"])
        run("rmdir probe-arbre", 28, ["non vide"])
        run("rmdir --verbose probe-vide", contains=["retiré probe-vide"])
        run("rmdir temoin", 29)
        run("rm --recursive --verbose probe-arbre", contains=["retiré probe-arbre/alpha","retiré probe-arbre"])
        run("ls -d probe-arbre", 20)
        run("rm -i absent", 20, absent=["[o/N]"])
        run("rm -i g", 30, absent=["[o/N]"])
        run("rm -i colonnes/a", answers=["n"])
        out = run("ls -l colonnes")
        rows = re.findall(r"^fichier +([0-9]+) +(inconnue +[ab])\r?$", out, re.M)
        assert rows == [("1","inconnue  a"), ("12","inconnue  b")], rows
        lines = [line for line in out.splitlines() if line.startswith("fichier ")]
        assert len(lines) == 2 and lines[0].index("inconnue") == lines[1].index("inconnue")
        assert lines[0].index("1") == lines[1].index("2")
        run("rm -i colonnes/a", answers=["o"])
        run("rm absent colonnes/b", 20, ["lot interrompu après 0"])
        run("cat colonnes/b", contains=["douze octets"])
        run("rm -rf absent")
        run("rm -f")
        run("rm -ri coupure", answers=["n"])
        run("cat coupure/beta", contains=["second"])
        run("cd g/src/lib", None)
        for name in [".","..","/"]:
            run("rm -rf "+name, 5)
        run("rm -rf /g", 24)
        run("cat test2.rs", contains=["imbriqué"])
        run("cd /", None)
        run("mkdir vide-a")
        run("mkdir vide-b")
        run("rmdir vide-a vide-b")
        run("rm -Rv archive", contains=["retiré archive/src/lib/profond/test3.rs","retiré archive/é.txt"])
        run("ls -d archive", 20)
        run("rm -Rf g/-nom")
        run("cat -- g/-nom", 20)
        run("cat temoin", contains=["intact"])
        # Interrompre seulement après le premier retrait acquitté, au second dialogue.
        t.vm.stdin.write(b"rm -riv coupure\n")
        for prompt in (b"Descendre dans coupure ? [o/N] ",b"Retirer coupure/alpha ? [o/N] "):
            t.until(prompt)
            t.vm.stdin.write(b"o\n")
        out = t.until(b"Retirer coupure/beta ? [o/N] ")
        assert "retiré coupure/alpha".encode() in out
    with image.open("rb") as source:
        subprocess.run([HOST, "verify", "512", str(sector), fs, lv], stdin=source, check=True)
    with Terminal(arch,sector,image,"nsh-remontage",test=True) as t:
        t.until(b"nsh> ")
        t.line("cat coupure/alpha", status="sortie 20")
        t.line("cat coupure/beta", status="sortie 0", has=["second"])
        t.line("cat temoin", status="sortie 0", has=["intact"])
        t.exit()
    return dict(architecture=arch, secteur=sector, commandes=len(checked), coupure="alpha retiré, beta et témoin intègres")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--sector", type=int, choices=[512,4096])
    opt = parser.parse_args()
    arch = os.environ["NOVA_ARCH"]
    for sector in [opt.sector] if opt.sector else [512,4096]:
        with tempfile.TemporaryDirectory(prefix="nova-nsh-") as tmp:
            result = check(arch, sector, Path(tmp))
            (ROOT/"build"/f"nsh-{arch}-{sector}.json").write_text(json.dumps(result, ensure_ascii=False)+"\n")
            print(result, flush=True)
