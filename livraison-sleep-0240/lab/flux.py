"""Vérifie l’offset de stdin, le retour avant EOF et la reprise après SIGCONT."""

import os
import signal
import subprocess as sp
import tempfile
import time


def measure(executable):
    env = dict(os.environ, LC_ALL="C")
    facts = []
    for args in (["0.002"], ["--help"], ["invalide"]):
        with tempfile.TemporaryFile(dir=os.environ["TMPDIR"]) as source:
            source.write(b"0123456789")
            source.seek(3)
            run = sp.run(
                ["sleep", *args],
                executable=executable,
                env=env,
                stdin=source,
                capture_output=True,
                timeout=2,
            )
            assert source.tell() == 3 and run.returncode == int(args == ["invalide"])
            facts.append(
                dict(
                    argv=args,
                    offset_avant=3,
                    offset_apres=source.tell(),
                    rc=run.returncode,
                )
            )
    rd, wr = os.pipe()
    try:
        os.write(wr, b"entree temoin")
        run = sp.run(
            ["sleep", ".002"],
            executable=executable,
            env=env,
            stdin=rd,
            capture_output=True,
            timeout=2,
        )
        assert run.returncode == 0 and not (run.stdout or run.stderr)
        os.close(wr)
        wr = None
        data = os.read(rd, 128)
        assert data == b"entree temoin"
        facts.append(
            dict(tube="sortie avant EOF", stdin_restant=data.hex(), rc=run.returncode)
        )
    finally:
        os.close(rd)
        if wr is not None:
            os.close(wr)
    start = time.monotonic_ns()
    with sp.Popen(
        ["sleep", ".15"],
        executable=executable,
        env=env,
        stdin=sp.DEVNULL,
        stdout=sp.PIPE,
        stderr=sp.PIPE,
    ) as child:
        try:
            time.sleep(0.025)
            child.send_signal(signal.SIGSTOP)
            time.sleep(0.020)
            child.send_signal(signal.SIGCONT)
            stdout, stderr = child.communicate(timeout=2)
        except BaseException:
            child.kill()
            child.wait()
            raise
        elapsed = time.monotonic_ns() - start
        assert (
            child.returncode == 0
            and not (stdout or stderr)
            and 150e6 <= elapsed <= 400e6
        )
        facts.append(
            dict(
                signaux=["SIGSTOP", "SIGCONT"],
                demande_s=0.15,
                ns=elapsed,
                rc=child.returncode,
            )
        )
    return facts
