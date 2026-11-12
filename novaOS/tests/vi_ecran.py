"""Contrôle des cellules et du curseur à partir du flux réel de l’invité, en mode Terminal VT."""
import os
from pathlib import Path
import re
import tempfile

from cli import Terminal
from vi import fixture


def cup(row, col):
    return f"\x1b[{row:03};{col:03}H".encode()


class Ecran:
    def __init__(self, rows, cols):
        self.rows, self.cols = rows, cols
        self.lines = [" " * cols for _ in range(rows)]
        self.row = self.col = 0

    def feed(self, raw):
        # Le banc exerce ici du texte à largeur d'une cellule. Toute autre commande échoue.
        dirty = set()
        for part in re.findall(r"\x1b\[[0-9;]*[A-Za-z]|[^\x1b]+|\x1b", raw.decode()):
            move = re.fullmatch(r"\x1b\[(\d+);(\d+)H", part)
            if move:
                self.row, self.col = (int(n)-1 for n in move.groups())
                assert 0 <= self.row < self.rows and 0 <= self.col < self.cols
            elif part == "\x1b[K":
                self.lines[self.row] = self.lines[self.row][:self.col].ljust(self.cols)
                dirty.add(self.row)
            else:
                assert all(c.isprintable() for c in part), repr(part)
                end = self.col + len(part)
                assert end <= self.cols, "retour automatique"
                line = self.lines[self.row]
                self.lines[self.row] = line[:self.col] + part + line[end:]
                self.col = end
                dirty.add(self.row)
        return dirty


def trial(arch, folder):
    image, _, _ = fixture(folder, 512)
    with Terminal(arch, 512, image, "vi-ecran") as term:
        write = term.vm.stdin.write
        term.until(b"nsh> ")
        write(b"vi toto\r")
        term.until(b"\x1b[?1049h")
        query = b"\x1b7\x1b[999;999H\x1b[6n\x1b8"
        assert term.until(query) == query
        # Une frappe précoce précède le CPR. Le courtier doit la rendre à vi.
        write(b"i\x1b[12;48R")
        screen = Ecran(12, 48)

        def frame(status, r, c):
            data = term.until(status.encode()) + term.until(cup(r, c))
            dirty = screen.feed(data)
            assert (screen.row, screen.col) == (r-1, c-1)
            return dirty

        frame("vi [NORMAL] toto [nouveau] 1:1", 1, 1)
        frame("vi [INSERTION] toto [nouveau] 1:1", 1, 1)
        word = "abcdefghijklmnopqrst"
        for i, ch in enumerate(word, 1):
            write(ch.encode())
            assert frame(f"toto [modifié] 1:{i+1}", 1, i+1) == {0, 10}
            assert screen.lines[0].rstrip() == word[:i]
            assert all(s.rstrip() == "~" for s in screen.lines[1:10])
            assert not screen.lines[-1].strip()
        write(b"\x08")
        frame("toto [modifié] 1:20", 1, 20)
        assert screen.lines[0].rstrip() == word[:-1]
        for i in range(1, 21):
            write(b"\r" + str(i).encode())
            frame(f"toto [modifié] {i+1}:{1+len(str(i))}", min(i+1, 10), 1+len(str(i)))
        assert [s.rstrip() for s in screen.lines[:10]] == list(map(str, range(11, 21)))
        write(b"\x1bgg")
        frame("toto [modifié] 1:1", 1, 1)
        assert screen.lines[0].rstrip() == word[:-1]
        # La requête périodique est provoquée par une vraie attente du lecteur.
        assert term.until(query) == query
        write(b"\x1b[8;36R")
        screen = Ecran(8, 36)
        frame("toto [modifié] 1:1", 1, 1)
        assert screen.lines[6].startswith("vi [NORMAL]") and screen.lines[0].rstrip() == word[:-1]
        write(b"\x0c")
        assert term.until(query) == query
        write(b"\x1b[1;1R")
        assert term.until(b"a"+cup(1,1)).endswith(b"a"+cup(1,1))
        write(b"\x0c")
        assert term.until(query) == query
        write(b"\x1b[8;36R")
        screen = Ecran(8, 36)
        frame("toto [modifié] 1:1", 1, 1)
        assert screen.lines[0].rstrip() == word[:-1]
        write(b":q!")
        screen.feed(term.until(cup(8, 4)))
        assert screen.lines[-1].rstrip() == ":q!"
        write(b"\r")
        term.until(b"\x1b[?1049l")
        term.until(b"nsh> ")
        term.exit()
    print(f"{arch} — 20 frappes, 20 lignes, trois géométries, lignes modifiées, curseur, retour à nsh", flush=True)


if __name__ == "__main__":
    with tempfile.TemporaryDirectory(prefix="nova-vi-ecran-") as tmp:
        trial(os.environ["NOVA_ARCH"], Path(tmp))
