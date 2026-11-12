"""À exécuter dans la copie instrumentée documentée. Toutes les images sont jetables."""
import os
import tempfile
from pathlib import Path
from cli import Terminal
from session_cli import fixture_p9


def trial(arch, sector, folder):
    image = folder / f"incident-{sector}.raw"
    fixture_p9(image, sector)
    with Terminal(arch, sector, image, "incidents") as t:
        t.until(b"nsh> ")
        for cmd in (
            "mkdir incident", "touch incident/avant.txt", "touch incident/concurrent",
            "mkdir manque", "touch manque/avant.txt", "touch manque/penurie",
        ):
            t.line(cmd, absent=[b"erreur", b"statut"])
        t.line("ls incident/**/*.txt", has=["capacité révoquée"])
        t.line("ls -d incident/nouveau", has=["\r\nincident/nouveau\r\n"])
        t.line("ls manque/**/*.txt", has=["mémoire insuffisante"])
        t.line("echo reprise", has=["\r\nreprise\r\n"])
        t.vm.stdin.write(b"cat /lignes\n")
        before = t.until(b"RENDEZVOUS-LECTURE\r\n")
        assert "début ".encode() in before and b"a" * 640 in before
        t.vm.stdin.write(b"r")
        after = t.until(b"nsh> ")
        assert any(message.encode() in after for message in ("accès refusé", "capacité révoquée")), after
        assert b"ligne 1" not in after and b"fin" not in after, after
        t.line("echo statut=$?", has=["statut="], absent=[b"statut=0\r\n"])
        t.exit()
    print(f"{arch}/{sector} — NT01, NT02, NT03 réussis", flush=True)


if __name__ == "__main__":
    source = Path(__file__).resolve().parents[2] / "components/coreutils/text.rs"
    if "RENDEZVOUS-LECTURE" not in source.read_text():
        raise SystemExit("Appliquer le patch de qualification dans une copie isolée du dépôt.")
    with tempfile.TemporaryDirectory(prefix="nsh-incidents-") as tmp:
        for size in (512, 4096):
            trial(os.environ["NOVA_ARCH"], size, Path(tmp))
