"""Route R2 hôte : distincte d'une preuve de lancement dans l'invité."""
from dataclasses import replace
import unittest
import administration_epochs as epochs
import admission_exec as execs
import admission_rpc as rpc
import reprise
import test_administration_epochs as fixtures

class Route(unittest.TestCase):
    def setUp(self):
        self.f = fixtures.Epochs()
        self.f.setUp()
        self.addCleanup(self.f.doCleanups)
        self.peer = self.f.server.public_key().public_bytes_raw()

    def send(self, operation, q=None, tail=b'', *, administration=True):
        f = self.f
        q = q or f.ticket
        prefix = {rpc.ADMIT: execs.ADMIT, rpc.FINISH: execs.FINISH, rpc.QUERY: execs.QUERY}[operation]
        if operation == rpc.FINISH:
            signed = q.finish(tail[0], tail[1:])
        else:
            signed = q.packet() + tail
        body = b''.join(f.scope) + f.incarnation + q.packet() + tail + f.server.sign(prefix + signed)
        return rpc.request(f.path, f.key, self.peer, f.scope, operation, body, administration=administration)

    def test_epoch_retired_stays_refused_after_reactivation(self):
        f = self.f
        # Ticket non consommé. Désactivation et clôture acquittée, puis réactivation.
        f.disable(); f.ack(); f.api.drain(*f.scope, b'R'*16); f.enable()
        current = f.current()
        old = replace(f.ticket, generation=current.generation, head=current.head)
        with self.assertRaisesRegex(reprise.Refus, 'époque'):
            self.send(rpc.ADMIT, old)
        # Seul le chemin de confiance fournira cette nouvelle époque au ticket.
        # Ce témoin hôte ne constitue pas une authentification de l'invité.
        fresh = replace(old, session=b'J'*32, authentication_epoch=current.generation)
        reply = self.send(rpc.ADMIT, fresh)
        f.key.public_key().verify(reply[-64:], execs.RECEIPT+reply[:-64])
        with self.assertRaisesRegex(reprise.Refus, 'déjà décidée'):
            self.send(rpc.ADMIT, fresh)
        with reprise.transaction(f.path) as db:
            self.assertEqual(db.execute('SELECT count(*) FROM exec_admissions').fetchone(), (1,))
            self.assertEqual(db.execute('SELECT epoch,state FROM subject_epochs WHERE subject=? ORDER BY epoch', (f.alice,)).fetchall(), [(2,execs.RETIRED),(8,execs.ACTIVE)])

    def test_delayed_creation_keeps_retirement_pending_and_query_never_repeats(self):
        f = self.f
        self.send(rpc.ADMIT)  # réponse perdue
        f.disable(); f.ack()
        with self.assertRaisesRegex(reprise.Refus, 'lancement'):
            f.api.drain(*f.scope,b'R'*16)
        first = self.send(rpc.QUERY, tail=b'F'*32)
        self.send(rpc.FINISH, tail=bytes([execs.CANCELLED])+bytes(32))
        terminal = self.send(rpc.QUERY, tail=b'G'*32)
        self.assertNotEqual(first, terminal)
        # R2 progresse après la preuve terminale authentifiée, pas sur QUERY.
        with self.assertRaisesRegex(reprise.Refus, 'terminé'):
            f.api.drain(*f.scope,b'R'*16)
        with reprise.transaction(f.path) as db:
            self.assertEqual(db.execute("SELECT state FROM subject_epochs WHERE subject=? AND epoch=2",(f.alice,)).fetchone(),(execs.RETIRED,))
            self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='admission'").fetchone(), (1,))
            self.assertEqual(db.execute("SELECT count(*) FROM exec_events WHERE kind='cancelled'").fetchone(), (1,))

    def test_no_downgrade_no_implicit_activation_no_substituted_subject(self):
        f = self.f
        with self.assertRaises(reprise.Refus):
            self.send(rpc.ADMIT, administration=False)
        old = execs.Admission(**{k:getattr(f.ticket,k) for k in execs.Admission.__dataclass_fields__})
        with self.assertRaisesRegex(reprise.Refus, 'époque'):
            self.send(rpc.ADMIT, old)
        q=f.ticket
        body=b''.join(f.scope)+f.incarnation+q.packet()+f.server.sign(execs.ADMIT+q.packet())
        for offset in (0,16,32,72,88,104,320,336,343):
            wrong=bytearray(body);wrong[offset]^=1
            with self.assertRaises(reprise.Refus):
                rpc.request(f.path,f.key,self.peer,f.scope,rpc.ADMIT,bytes(wrong),administration=True)
        for wrong in (body[:-1],body+b'0',body[:320]+body[-64:]):
            with self.assertRaises(reprise.Refus):
                rpc.request(f.path,f.key,self.peer,f.scope,rpc.ADMIT,wrong,administration=True)
        with reprise.transaction(f.path) as db:
            self.assertEqual(db.execute('SELECT count(*) FROM exec_admissions').fetchone(), (0,))

if __name__ == '__main__': unittest.main()
