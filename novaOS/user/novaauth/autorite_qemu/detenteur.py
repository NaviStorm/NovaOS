"""Journal indépendant d'un détenteur QEMU, avec reprise explicite du chiffré."""
from contextlib import closing
from pathlib import Path
import os
import sqlite3
import subprocess

from recuperation import Context, Refus, verify, digest, QUERY, RESERVE, ADMIN, SERVER_BINDING, POSSESSION
from reprise import fixed

SCHEMA = """
CREATE TABLE holder(role INTEGER PRIMARY KEY CHECK(role IN (1,2)),
    public_key BLOB NOT NULL, signing_key BLOB NOT NULL);
CREATE TABLE envelopes(operation BLOB PRIMARY KEY, context BLOB NOT NULL,
    delivery BLOB UNIQUE NOT NULL, nonce BLOB NOT NULL, state TEXT NOT NULL,
    ciphertext BLOB, hash BLOB);
"""

def initialize(path, role, public, signing):
    if role not in (1,2):
        raise Refus("rôle inconnu")
    fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
    os.close(fd)
    with closing(connect(path)) as db:
        db.executescript("BEGIN IMMEDIATE;"+SCHEMA)
        db.execute("INSERT INTO holder VALUES(?,?,?)",(role,fixed(public,32),fixed(signing,32)))
        db.commit()

def connect(path):
    db=sqlite3.connect(Path(path).resolve().as_uri()+"?mode=rw",uri=True,isolation_level=None)
    db.execute("PRAGMA synchronous=EXTRA")
    db.execute("PRAGMA fullfsync=ON")
    return db

class Holder:
    def __init__(self,path,role,public,signing_public,signing_key,key_path,signing_path,executable,
                 authority_public,server_public,admin_public,measurements):
        self.path=path
        self.role=role
        self.public=public
        self.signing=signing_public
        self.sign=signing_key
        self.key_path=Path(key_path)
        self.signing_path=Path(signing_path)
        self.executable=Path(executable)
        self.authority=authority_public
        self.server=server_public
        self.admin=admin_public
        self.measurements=frozenset(measurements)
        with closing(connect(path)) as db:
            if db.execute("SELECT role,public_key,signing_key FROM holder").fetchall()!=[(role,public,signing_public)]:
                raise Refus("identité du détenteur divergente")

    def _check(self,c,manifest,server_signature,admin_signature,possession,observation,signature,fresh):
        # Une réponse à un nonce choisi par CE détenteur est exigée pour chaque
        # émission et récupération explicite. Le transport appelant ne peut le choisir.
        packet=c.packet()
        if c.measurement not in self.measurements or c.manifest!=digest(manifest):
            raise Refus("cible ou sauvegarde non autorisée")
        if len(manifest)!=418 or manifest[94+32*(self.role-1):126+32*(self.role-1)]!=digest(self.public):
            raise Refus("clé de sauvegarde incompatible")
        if manifest[78:86]!=c.generation.to_bytes(8,"big") or manifest[86:94]!=c.key_epoch.to_bytes(8,"big"):
            raise Refus("époque de sauvegarde incompatible")
        verify(self.server,SERVER_BINDING,packet,server_signature)
        verify(self.admin,ADMIN,packet,admin_signature)
        verify(c.possession,POSSESSION,packet,possession)
        if observation!=c.hash()+fresh:
            raise Refus("observation périmée ou substituée")
        verify(self.authority,QUERY,observation,signature)

    def deliver(self,c,manifest,delivery,nonce,server_signature,admin_signature,possession,
                current,reservation,reservation_signature,cut=None,request=None):
        # current est un client authentifié de l'autorité. Il doit relayer la
        # requête du destinataire et retourner la preuve fraîche de l'autorité.
        import secrets
        fresh=secrets.token_bytes(32)
        observation,signature=current(c.op,fresh)
        self._check(c,manifest,server_signature,admin_signature,possession,observation,signature,fresh)
        expected=c.hash()+bytes([self.role])+fixed(delivery,16)+fixed(nonce,32)
        if reservation!=expected:
            raise Refus("réservation substituée")
        verify(self.authority,RESERVE,expected,reservation_signature)
        with closing(connect(self.path)) as db:
            db.execute("BEGIN IMMEDIATE")
            try:
                if db.execute("SELECT 1 FROM envelopes WHERE operation=?",(c.op,)).fetchone():
                    raise Refus("remise déjà engagée, consulter son issue")
                db.execute("INSERT INTO envelopes VALUES(?,?,?,?,?,NULL,NULL)",
                           (c.op,c.packet(),delivery,nonce,"reserved"))
                db.commit()
            except BaseException:
                db.rollback()
                raise
        if cut=="reserved":
            raise Refus("UNCERTAIN après réservation locale")
        # Ne passe aucun secret dans argv, stdin ou stdout.
        result=self._seal(c,manifest,delivery,nonce,request)
        if result.returncode or len(result.stdout)!=324:
            raise Refus("échec de scellement, aucune répétition automatique")
        envelope=result.stdout
        verify(self.signing,b"",envelope[:260],envelope[260:])
        with closing(connect(self.path)) as db:
            db.execute("BEGIN IMMEDIATE")
            db.execute("UPDATE envelopes SET state='sealed',ciphertext=?,hash=? WHERE operation=? AND state='reserved'",
                       (envelope,digest(envelope),c.op))
            db.commit()
        if cut=="sealed":
            raise Refus("UNCERTAIN après scellement durable")
        # Une coupure de processus entre le scellement et cette écriture conserve
        # le chiffré exact. Aucun nouvel encapsulage n'est autorisé.
        return envelope,self.sign.sign(RESERVE+c.hash()+digest(envelope))

    def _seal(self,c,manifest,delivery,nonce,request):
        return subprocess.run([str(self.executable),str(self.role),str(self.key_path),str(self.signing_path)],
                              input=manifest+c.hash()+delivery+nonce+c.receiver,
                              stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=False,timeout=10)

    def outcome(self,c,manifest,server_signature,admin_signature,possession,current,retrieve=False):
        import secrets
        fresh=secrets.token_bytes(32)
        observation,signature=current(c.op,fresh)
        self._check(c,manifest,server_signature,admin_signature,possession,observation,signature,fresh)
        with closing(connect(self.path)) as db:
            row=db.execute("SELECT context,state,ciphertext,hash FROM envelopes WHERE operation=?",(c.op,)).fetchone()
        if row is None or row[0]!=c.packet():
            raise Refus("remise inconnue")
        # Une réservation interrompue ne se transforme pas implicitement en remise.
        if not retrieve:
            return row[1],row[3]
        if row[1]!="sealed" or digest(row[2])!=row[3]:
            raise Refus("remise indisponible, nouvelle procédure administrative nécessaire")
        return row[2],self.sign.sign(RESERVE+c.hash()+row[3])
