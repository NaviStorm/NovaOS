"""Relecture en lecture seule des traces noyau et du journal individuel. Aucune opération rejouée."""
from pathlib import Path
import hashlib,json,re,sqlite3,sys
path=Path(sys.argv[1]).resolve();lab=Path(sys.argv[2]).resolve()
sys.path.insert(0,str(lab))
from administration_outputs import Collector,WIRE
def require(ok,why):
    if not ok: raise ValueError(why)
def sha(b):return hashlib.sha256(b).hexdigest()
raw=(path/'invite/sessions-noyau.bin').read_bytes()
require(len(raw)==160,'deux sessions attendues')
sessions=[]
for i in (0,80):
    record=raw[i:i+80]
    require(record[:8]==b'NXR2SES1','identité de session')
    sessions.append((int.from_bytes(record[8:16],'little'),record[16:48],record[48:]))
collector=Collector(sessions)
frames=(path/'invite/sorties-noyau.bin').read_bytes()
require(len(frames)%WIRE==0,'trame tronquée')
for at in range(0,len(frames),WIRE): collector.accept(frames[at:at+WIRE])
trace=(path/'invite/invite.serial').read_bytes()
def unique(pattern):
    matches=re.findall(pattern,trace)
    require(len(matches)==1,'événement manquant ou dupliqué '+repr(pattern))
    return matches[0]
pid=unique(rb'DEBUG pid=(\d+) NAR2 Bob ticket vivant non consomme\r?\n')
child=unique(rb'NAEXEC enfant owner='+pid+rb' child=(\d+) ticks=\d+')
now,deadline=map(int,unique(rb'NAEXEC attente owner='+pid+rb' ticket=\d+ maintenant=(\d+) echeance=(\d+)'))
require(now<deadline,'ticket hors délai')
unique(rb'DEBUG pid='+pid+rb' NAR2 Bob ancien H consomme statut=0 collecte unique\r?\n')
unique(rb'DEBUG pid='+pid+rb' NAR2 Bob fermeture acquittee\r?\n')
rows=[s for s in collector.streams.values() if s.binding.child==int(child)]
require(len(rows)==2 and all(s.closed and s.binding.shell==int(pid) for s in rows),'propriétaire des sorties')
require(len({s.binding.child_instance for s in rows})==1 and len({s.binding.endpoint for s in rows})==2,'identité enfant et extrémités distinctes')
expected=b'T:A\tB\tC\nD\tE\tF\n'
require({s.binding.fd:bytes(s.data) for s in rows}=={1:expected,2:b''},'contenu des sorties')
child_instance=rows[0].binding.child_instance
unique(rb'NAEXEC restitue pid='+pid+rb' roots=0 handles=0 waits=0 pages=0 pile=0 threads=0 ')
unique(rb'NAEXEC restitue pid='+child+rb' roots=0 handles=0 waits=0 pages=0 pile=0 threads=0 ')
require(b'endpoints=0 nodes=0 espaces=0 cadres_ipc=0' in trace,'drainage final')
unique(rb'NXDUPLEX phase=libre child='+child+rb' lane=[01]\r?\n')
samples=re.findall(rb'NXDUPLEX phase=(\w+) child='+child+rb' roots=(\d+) max_roots=(\d+) endpoints=(\d+) nodes=(\d+) handles=(\d+) waits=(\d+) cadres_ipc=(\d+) cadres_libres=Ok\((\d+)\)',trace)
require([(r[0],int(r[1])) for r in samples]==[(b'sortie',1),(b'erreur',2),(b'fichier_tx',3),(b'fichier_rx',4),(b'actif',4)] and max(int(r[2]) for r in samples)<=4,'budget de racines')
dbfile=path/'autorite/autorite-independante.sqlite'
db=sqlite3.connect(dbfile.as_uri()+'?mode=ro',uri=True)
db.execute('BEGIN')
(op,sub,grp,generation,state,p9),=db.execute('select op,subject,grp,generation,state,p9 from membership_changes')
require((state,p9)==(2,1),'retrait non terminal')
eff=list(db.execute('select kind,acknowledged from membership_effectors where op=? order by kind',(op,)))
require(eff==[(1,1),(2,1),(4,1)],'ACK classes')
intent,=re.findall(rb'DEBUG pid='+pid+rb' NAR2 Bob declenchement operation=([0-9a-f]{32})',trace)
require(op.hex()==intent.decode(),'opération administrative')
(r,pu,reti)=list(db.execute("select sequence,kind from exec_events where kind in ('membership-reserved','membership-withdrawing','membership-retired') order by sequence"))
require([q[1] for q in (r,pu,reti)]==['membership-reserved','membership-withdrawing','membership-retired'],'ordre retrait')
(exec_op,pkt,register,admit,terminal,st,ident),=db.execute('select d.op,d.packet,d.sequence,a.sequence,a.terminal_sequence,a.state,a.child from exec_descriptors d join exec_admissions a on d.op=a.op where d.subject=?',(sub,))
require(st==1 and ident==child_instance and register<r[0]<pu[0]<reti[0]<admit<terminal,'identité et ordre EXEC')
group=pkt[616:632]
require(len(pkt)==656 and group not in (grp,bytes(16)),'portée H')
reads=list(db.execute('select op,grp,kind,state,sequence,terminal_sequence,evidence from membership_operations where subject=? order by sequence',(sub,)))
require(len(reads)==4 and len({row[0] for row in reads})==4 and all(g==group and k==st==1 and terminal<a<f and ev and len(ev)==32 for _,g,k,st,a,f,ev in reads),'issues VFS H')
relevant=[l for l in trace.splitlines() if l.startswith((b'NXDUPLEX phase=',b'NAEXEC ressources ',b'NAEXEC restitue '))]
free=[int(x) for l in relevant for x in re.findall(rb'cadres_libres=Ok\((\d+)\)',l)]
ipc=[int(x) for l in relevant for x in re.findall(rb'cadres_ipc=(\d+)',l)]
result=dict(child=int(child),shell=int(pid),child_instance_sha256=sha(ident),operation=exec_op.hex(),ticket_observe=now,echeance=deadline,marge_ticks=deadline-now,statut=0,stdout_octets=len(expected),stdout_sha256=sha(expected),stderr_octets=0,sessions_distinctes=True,sequence_retrait=[r[0],pu[0],reti[0]],generation=generation,acquittements=eff,inscription=register,admission=admit,terminal=terminal,issues_vfs=[dict(id=row[0].hex(),admission=row[4],terminal=row[5],preuve=row[6].hex()) for row in reads],allocations=[dict(zip(('phase','racines','max_racines_acteurs','endpoints','noeuds','handles','attentes','cadres_ipc','cadres_libres'),[row[0].decode(),*map(int,row[1:])])) for row in samples],cadres_ipc_max_observe=max(ipc),cadres_libres_min_observe=min(free),drainage_final=True,traces_sha256={p.name:sha(p.read_bytes()) for p in [path/'invite/sessions-noyau.bin',path/'invite/sorties-noyau.bin',path/'invite/invite.serial',dbfile]})
print(json.dumps(result,ensure_ascii=False,indent=2))