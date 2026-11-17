"""Événements Darwin qui exigent un processus vivant ou une limite du système."""

import json
import os
from pathlib import Path
import resource
import select
import signal
import subprocess as sp
import tempfile
import time
from options import GNU, NOVA, b64


def probe(exe, root):
    env = dict(os.environ, LC_ALL="C")
    rows = {}

    def save(name, p, out, err):
        rows[name] = dict(rc=p.returncode, stdout=b64(out), stderr=b64(err))

    r, w = os.pipe()
    try:
        for conv, padded in [("", False), ("conv=noerror,sync", True)]:
            cmd = ["dd", "bs=4", "count=1", "iflag=nonblock", "status=none"] + (
                [conv] if conv else []
            )
            p = sp.run(
                cmd, executable=exe, env=env, stdin=r, capture_output=True, timeout=3
            )
            assert p.returncode == (0 if padded else 1)
            assert p.stdout == (bytes(4) if padded else b"")
            assert (
                (not p.stderr)
                if padded
                else b"Resource temporarily unavailable" in p.stderr
            )
            save("EAGAIN " + conv, p, p.stdout, p.stderr)
    finally:
        os.close(r)
        os.close(w)
    src = root / "source"
    src.write_bytes(bytes(range(256)) * 64)

    def limit():
        resource.setrlimit(resource.RLIMIT_FSIZE, (6144, 6144))
        signal.signal(signal.SIGXFSZ, signal.SIG_IGN)

    p = sp.run(
        ["dd", "if=source", "of=limite", "bs=4096", "status=noxfer"],
        executable=exe,
        cwd=root,
        env=env,
        capture_output=True,
        preexec_fn=limit,
        timeout=3,
    )
    assert (
        p.returncode == 1
        and b"1+1 records out" in p.stderr
        and (root / "limite").read_bytes() == src.read_bytes()[:6144]
    ), (exe, p.stderr)
    save("écriture partielle", p, p.stdout, p.stderr)
    p = sp.Popen(
        ["dd", "bs=1", "status=noxfer"],
        executable=exe,
        stdin=sp.PIPE,
        stdout=sp.PIPE,
        stderr=sp.PIPE,
        env=env,
    )
    try:
        p.stdin.write(b"A")
        p.stdin.flush()
        assert select.select([p.stdout], [], [], 3)[0] and p.stdout.read(1) == b"A"
        os.kill(p.pid, signal.SIGINFO)
        assert select.select([p.stderr], [], [], 3)[0], "SIGINFO pendant la lecture"
        reported = p.stderr.readline() + p.stderr.readline()
        assert reported == b"1+0 records in\n1+0 records out\n", reported
        os.kill(p.pid, signal.SIGINT)
        out, err = p.communicate(timeout=3)
        assert p.returncode == -signal.SIGINT and not out
        save("SIGINFO puis SIGINT", p, b"A" + out, reported + err)
    finally:
        if p.poll() is None:
            p.kill()
            p.wait()
    p = sp.Popen(
        ["dd", "bs=1", "status=progress"],
        executable=exe,
        stdin=sp.PIPE,
        stdout=sp.PIPE,
        stderr=sp.PIPE,
        env=env,
    )
    try:
        time.sleep(1.1)
        p.stdin.write(b"X")
        p.stdin.flush()
        assert select.select([p.stdout], [], [], 3)[0] and p.stdout.read(1) == b"X"
        out, err = p.communicate(b"Y", timeout=3)
        assert (
            p.returncode == 0
            and out == b"Y"
            and b"\r1 byte copied" in err
            and b"2 bytes copied" in err
        ), err
        save("status=progress", p, b"X" + out, err)
    finally:
        if p.poll() is None:
            p.kill()
            p.wait()
    size = 4 * 1024**2
    assert __import__("shutil").disk_usage(root).free > 16 * 1024**2
    for mode in ["sparse", "notrunc"]:
        p = sp.run(
            [
                "dd",
                "if=/dev/zero",
                "of=" + mode,
                "count=4",
                "bs=1M",
                "conv=" + mode,
                "status=none",
            ],
            executable=exe,
            env=env,
            cwd=root,
            capture_output=True,
            check=True,
        )
        dest = root / mode
        assert dest.stat().st_size == size and not any(dest.read_bytes())
        rows[mode] = dict(octets=size, blocs512=dest.stat().st_blocks, rc=p.returncode)
    assert rows["sparse"]["blocs512"] < rows["notrunc"]["blocs512"]
    return rows


if __name__ == "__main__":
    import sys

    result = {}
    for label, exe in [("Nova", NOVA), ("GNU", GNU)]:
        with tempfile.TemporaryDirectory(
            dir=os.environ["TMPDIR"], prefix="dd-flux-"
        ) as tmp:
            result[label] = probe(exe, Path(tmp))
    Path(sys.argv[1]).write_text(json.dumps(result, indent=2) + "\n")
    print("Événements Darwin vérifiés pour Nova et GNU")
