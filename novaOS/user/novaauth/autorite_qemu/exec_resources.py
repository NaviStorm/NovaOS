"""Mesures invitées : distinguer compteurs globaux et ressources par acteur."""
import re
from reprise import Refus
GLOBAL=rb'NAEXEC ressources phase=(\w+) owner=(\d+) roots=(\d+) handles=(\d+) endpoints=(\d+) nodes=(\d+) waits=(\d+) processus=(\d+) threads=(\d+) joins=(\d+) cadres_ipc=(\d+) cadres_libres=Ok\((\d+)\)\r?\n'
ACTOR=rb'NAEXEC acteur phase=(\w+) owner=(\d+) pid=(\d+) service=(\d+) roots=(\d+) handles=(\d+) waits=(\d+) pages=(\d+) pile=(\d+) threads=(\d+) cadres_ipc=(\d+)\r?\n'
FIELDS=('owner','roots','handles','endpoints','nodes','waits','processus','threads','joins','cadres_ipc','cadres_libres')
AFIELDS=('owner','pid','service','roots','handles','waits','pages','pile','threads','cadres_ipc')
def snapshots(raw):
    globals=[dict(phase=m[0].decode(),**dict(zip(FIELDS,map(int,m[1:])))) for m in re.findall(GLOBAL,raw)]
    actors=[dict(phase=m[0].decode(),**dict(zip(AFIELDS,map(int,m[1:])))) for m in re.findall(ACTOR,raw)]
    return globals,actors

def verify(raw,probe=False,warm=False):
    g,a=snapshots(raw)
    first='consommation' if probe else 'avant_ticket'
    last='refus_sans_creation' if probe else 'fermeture'
    before=[r for r in g if r['phase']==first];after=[r for r in g if r['phase']==last]
    if not before or len(before)!=len(after):raise Refus('mesures initiales/finales incomplètes')
    if not probe and not any(r['phase']=='consommation' for r in g):raise Refus('mesure pendant lancement absente')
    for left,right in zip(before,after):
        for field in ('owner','roots','handles','endpoints','nodes','processus','threads','joins'):
            if left[field]!=right[field]:raise Refus('restitution globale différente : '+field)
        growth=right['cadres_ipc']-left['cadres_ipc']
        if growth<0 or growth>(0 if probe else 1):raise Refus('croissance pool IPC hors référence')
        if right['cadres_libres']+growth<left['cadres_libres']:raise Refus('cadres mémoire non restitués')
    # Les attentes globales varient avec les services indépendants. Les attentes
    # propres à chacun des trois acteurs restent bornées par son unique thread.
    ids={r['pid'] for r in a}
    if len(ids)!=3 or {r['service'] for r in a if r['pid']!=r['owner']}!={22,23}:
        raise Refus('mesures registre/signataire/shell absentes')
    for pid in ids:
        start=[r for r in a if r['pid']==pid and r['phase']==first]
        end=[r for r in a if r['pid']==pid and r['phase']==last]
        if len(start)!=len(before) or len(end)!=len(after):raise Refus('mesures acteur incomplètes')
        for left,right in zip(start,end):
            for field in ('roots','handles','pages','pile','threads'):
                if left[field]!=right[field]:raise Refus('restitution acteur différente : '+str(pid)+' '+field)
        if any(r['roots']>4 or r['waits']>r['threads'] for r in a if r['pid']==pid):raise Refus('quota acteur dépassé')
    if warm:
        if len(after)!=2:raise Refus('deux lancements B requis pour mesurer le plateau')
        for field in ('cadres_ipc','cadres_libres','endpoints','nodes','processus','threads','joins'):
            if after[0][field]!=after[1][field]:raise Refus('état chaud non stable : '+field)
    return {'avant':before,'pendant':[r for r in g if r['phase']=='consommation'],'apres':after,'acteurs':a,'restitution_verifiee':True,'plateau_chaud':warm,
            'reference':'racines/handles/objets/processus/threads identiques ; pool IPC au plus un cadre au premier lancement, plateau exact au second ; attentes propres bornées'}
