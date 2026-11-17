"""Contrôles de l'autorité externe sur des domaines et des clés jetables."""

from concurrent.futures import ThreadPoolExecutor
from dataclasses import replace
from pathlib import Path
from threading import Barrier
from tempfile import TemporaryDirectory
import sqlite3
import unittest

from cryptography.exceptions import InvalidSignature
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

from reprise import (
    ADMIN,
    RECEIPT,
    QUERY,
    HEAD_OBSERVED,
    HEAD_RECEIPT,
    Recovery,
    Refus,
    initialize,
)


class Reprise(unittest.TestCase):
    def setUp(self):
        self.tmp = TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name) / "externe.sqlite"
        self.owner, self.server, self.authority = [
            Ed25519PrivateKey.generate() for _ in range(3)
        ]
        self.clock = 100
        initialize(self.path, self.authority.public_key().public_bytes_raw())
        self.restart()
        self.pair = (b"S" * 16, b"D" * 16)
        self.old, self.head = b"I" * 32, b"H" * 32
        self.api.provision(
            *self.pair,
            self.old,
            5,
            self.head,
            self.server.public_key().public_bytes_raw(),
            self.owner.public_key().public_bytes_raw(),
        )

    def restart(self):
        self.api = Recovery(self.path, self.authority, lambda: self.clock)

    def issue(self, n=1):
        return self.api.issue(*self.pair, bytes([n]) * 32, bytes([n]) * 16)

    def signed(self, request):
        c = request[0]
        return (
            *request,
            self.owner.sign(ADMIN + c.packet()),
            self.server.sign(c.possession()),
        )

    def state(self):
        with sqlite3.connect(self.path) as db:
            return db.execute(
                "SELECT instance,epoch,generation,head FROM scope"
            ).fetchone()

    def test_identity_substitution_and_self_approval(self):
        request = self.issue()
        c, d, cookie, admin, guest = self.signed(request)
        for field, value in [
            ("server", b"A" * 16),
            ("domain", b"B" * 16),
            ("new", b"C" * 32),
            ("old", b"C" * 32),
            ("head", b"O" * 32),
            ("generation", 4),
            ("op", b"X" * 16),
        ]:
            with self.subTest(field=field), self.assertRaises(Refus):
                self.api.admit(replace(c, **{field: value}), d, cookie, admin, guest)
        for bad in (b"", self.server.sign(ADMIN + c.packet())):
            with self.assertRaises(InvalidSignature):
                self.api.admit(c, d, cookie, bad, guest)
        with self.assertRaises(InvalidSignature):
            self.api.admit(c, d, cookie, admin, self.owner.sign(c.possession()))
        self.assertEqual(self.state(), (self.old, 0, 5, self.head))

    def test_expiry_and_restart_before_decision(self):
        request = self.signed(self.issue())
        self.clock = request[1]
        with self.assertRaises(Refus):
            self.api.admit(*request)
        other = self.signed(self.issue(2))
        self.restart()
        with self.assertRaises(Refus):
            self.api.admit(*other)
        with self.assertRaises(Refus):
            self.api.outcome(other[0].op, b"N" * 32)
        self.assertEqual(self.state(), (self.old, 0, 5, self.head))

    def test_authority_identity_and_distinct_roles(self):
        keys = [
            key.public_key().public_bytes_raw()
            for key in (self.server, self.owner, self.authority)
        ]
        for server, admin in (
            (keys[0], keys[2]),
            (keys[2], keys[1]),
            (keys[0], keys[0]),
            (keys[1], keys[0]),
        ):
            with self.assertRaises(Refus):
                self.api.provision(
                    b"Z" * 16, b"E" * 16, self.old, 5, self.head, server, admin
                )
        with self.assertRaises(Refus):
            Recovery(self.path, self.server)

    def test_authority_crash_before_commit(self):
        # Arrêt brutal dans un processus distinct après les INSERT et UPDATE,
        # au passage de COMMIT, avant que SQLite ne publie la transaction.
        import os
        import subprocess
        import sys

        script = """
import os, sys
from pathlib import Path
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
import reprise
keys=[Ed25519PrivateKey.from_private_bytes(sys.stdin.buffer.read(32)) for _ in range(3)]
srv,admin,authority=keys
api=reprise.Recovery(Path(sys.argv[1]),authority)
c,deadline,cookie=api.issue(b'S'*16,b'D'*16,b'J'*32,b'K'*16)
original=reprise.open_database
def interrupted(path):
    db=original(path)
    def on_statement(sql):
        if sql == 'COMMIT': os._exit(71)
    db.set_trace_callback(on_statement)
    return db
reprise.open_database=interrupted
api.admit(c,deadline,cookie,admin.sign(reprise.ADMIN+c.packet()),srv.sign(c.possession()))
"""
        keys = (self.server, self.owner, self.authority)
        proc = subprocess.run(
            [sys.executable, "-B", "-c", script, str(self.path)],
            cwd=Path(__file__).parent,
            input=b"".join(k.private_bytes_raw() for k in keys),
        )
        self.assertEqual(proc.returncode, 71)
        self.restart()
        self.assertEqual(self.state(), (self.old, 0, 5, self.head))
        with self.assertRaises(Refus):
            self.api.outcome(b"K" * 16, b"F" * 32)
        self.api.admit(*self.signed(self.issue()))

    def test_lost_reply_restart_then_lookup_only(self):
        request = self.signed(self.issue())
        self.api.admit(*request)  # réponse jetée après commit
        self.restart()
        c = request[0]
        nonce = b"Z" * 32
        packet, signature, fresh = self.api.outcome(c.op, nonce)
        self.assertEqual(packet, c.packet())
        self.authority.public_key().verify(signature, RECEIPT + packet)
        self.authority.public_key().verify(fresh, QUERY + nonce + packet + signature)
        with self.assertRaises(Refus):
            self.api.admit(*request)
        with self.assertRaises(Refus):
            self.api.issue(*self.pair, self.old, b"R" * 16)
        self.assertEqual(self.state(), (c.new, 1, 5, self.head))
        with sqlite3.connect(self.path) as db:
            self.assertEqual(
                db.execute("SELECT op,admin FROM audit").fetchall(), [(c.op, c.admin)]
            )
            approval, possession = db.execute(
                "SELECT admin_signature,possession FROM audit"
            ).fetchone()
            self.owner.public_key().verify(approval, ADMIN + c.packet())
            self.server.public_key().verify(possession, c.possession())

    def test_rebind_keeps_revoked_policy_and_history(self):
        grant = b"G" * 32
        revoked = b"R" * 32
        get = lambda: self.api.observe(*self.pair, b"Q" * 32)[0]
        initial = get()
        first = initial.mutation(b"1" * 16, grant)
        grant_receipt = self.api.publish(
            initial, b"1" * 16, grant, self.server.sign(first)
        )
        snapshot = get()
        obsolete = self.signed(self.issue())
        revoke = snapshot.mutation(b"2" * 16, revoked)
        self.api.publish(snapshot, b"2" * 16, revoked, self.server.sign(revoke))
        with self.assertRaises(Refus):
            self.api.admit(*obsolete)
        self.api.admit(*self.signed(self.issue(3)))
        self.restart()
        state, signature = self.api.observe(*self.pair, b"F" * 32)
        self.authority.public_key().verify(
            signature, HEAD_OBSERVED + b"F" * 32 + state.packet()
        )
        self.assertEqual((state.head, state.generation, state.epoch), (revoked, 7, 1))
        historical, receipt, fresh = self.api.mutation_outcome(b"1" * 16, b"N" * 32)
        self.assertEqual((historical, receipt), (first, grant_receipt))
        self.authority.public_key().verify(receipt, HEAD_RECEIPT + historical)
        self.authority.public_key().verify(
            fresh, QUERY + b"N" * 32 + historical + receipt
        )
        stale = snapshot.mutation(b"3" * 16, grant)
        with self.assertRaises(Refus):
            self.api.publish(snapshot, b"3" * 16, grant, self.server.sign(stale))
        with self.assertRaises(InvalidSignature):
            self.authority.public_key().verify(
                receipt, HEAD_OBSERVED + b"F" * 32 + snapshot.packet()
            )
        self.assertEqual(get(), state)
        duplicate = state.mutation(bytes([3]) * 16, b"P" * 32)
        with self.assertRaises(Refus):
            self.api.publish(
                state, bytes([3]) * 16, b"P" * 32, self.server.sign(duplicate)
            )
        q = self.api.issue(*self.pair, b"L" * 32, b"2" * 16)
        with self.assertRaises(Refus):
            self.api.admit(*self.signed(q))

    def test_concurrent_administrative_decisions(self):
        both = [self.signed(self.issue(i)) for i in (1, 2)]
        start = Barrier(2)

        def approve(r):
            start.wait(timeout=5)
            try:
                self.api.admit(*r)
            except Refus:
                return False
            return True

        with ThreadPoolExecutor(2) as pool:
            result = list(pool.map(approve, both))
        self.assertEqual(result.count(True), 1)
        winner = both[result.index(True)][0]
        self.assertEqual(self.state(), (winner.new, 1, 5, self.head))
        with sqlite3.connect(self.path) as db:
            self.assertEqual(db.execute("SELECT COUNT(*) FROM audit").fetchone(), (1,))


if __name__ == "__main__":
    unittest.main()
