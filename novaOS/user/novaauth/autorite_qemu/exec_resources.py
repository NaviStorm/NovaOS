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
    full_raw=raw
    if not probe and b"NAEXEC ressources phase=avant_ticket" in raw:
        raw=raw[raw.index(b"NAEXEC ressources phase=avant_ticket"):]
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
        if len(after)!=warm or warm not in (3,6):raise Refus('trois ou six lancements B requis pour mesurer le plateau')
        for field in ('cadres_ipc','cadres_libres','endpoints','nodes','processus','threads','joins'):
            if any(end[field]!=after[1][field] for end in after[2:]):raise Refus('état chaud non stable : '+field)
    memory=memory_cycles(full_raw,warm) if warm else []
    return {'memoire_novafs':memory,'avant':before,'pendant':[r for r in g if r['phase']=='consommation'],'apres':after,'acteurs':a,'restitution_verifiee':True,'plateau_chaud':warm,
            'reference':'racines/handles/objets/processus/threads identiques ; pool IPC au plus un cadre au premier lancement, plateau exact après échauffement et cycles supplémentaires ; attentes propres bornées'}


def memory_cycles(raw,count):
    records=[list(int(v,16) for v in m.split()) for m in re.findall(rb'NXMEM3 ((?:[0-9a-f]{12} ){17})\r?\n',raw)]
    if len(records)!=count:raise Refus('mesures du tas NovaFS incomplètes')
    result=[]
    for i,v in enumerate(records):
        live,blocks,grow,returned,calls=v[:5]
        end_live,end_blocks,end_grow,end_returned,end_calls=v[5:10]
        cache,end_cache,budget=v[10:13]
        reused_before,bytes_before,reused_after,bytes_after=v[13:]
        if budget!=1065472 or max(cache,end_cache)>budget:raise Refus('budget cache historique dépassé')
        if end_live-end_cache>live-cache:raise Refus('allocations vivantes hors cache non restituées')
        if end_grow<grow or end_returned<returned or end_calls<=calls:raise Refus('compteurs allocation incohérents')
        reuse=end_grow==grow
        if i>=2 and (not reuse or reused_after<=reused_before or bytes_after<=bytes_before):raise Refus('réutilisation des dernières pages attribuées sans nouvelle attribution physique non démontrée')
        result.append({'cycle':i+1,'actifs_demandes_avant':live,'actifs_demandes_apres':end_live,
            'blocs_actifs_avant':blocks,'blocs_actifs_apres':end_blocks,'cache_avant':cache,'cache_apres':end_cache,'budget_cache':budget,
            'cadres_tas_avant':grow-returned,'cadres_tas_apres':end_grow-end_returned,
            'cadres_attribues':end_grow-grow,'cadres_restitues':end_returned-returned,
            'allocations_servies':end_calls-calls,'reutilisations_derniere_region':reused_after-reused_before if reuse else None,'octets_servis_derniere_region':bytes_after-bytes_before if reuse else None,'reutilisation_sans_attribution_physique':reuse,
            'octets_residents_non_demandes':(end_grow-end_returned)*4096-end_live,
            'limite':'Les octets non demandés incluent espace libre et métadonnées dlmalloc ; ce compteur seul ne prouve pas leur libération physique.'})
    return result


def retired_family(raw):
    family=[dict(zip(('manager','pid','service'),map(int,row))) for row in re.findall(
        rb'NAEXEC retrait_famille manager=(\d+) pid=(\d+) service=(\d+) statut=125\r?\n',raw)]
    fields=('pid','roots','handles','waits','pages','pile','threads','endpoints','nodes','espaces','cadres_ipc','cadres_libres')
    values=[dict(zip(fields,map(int,row))) for row in re.findall(
        rb'NAEXEC restitue pid=(\d+) roots=(\d+) handles=(\d+) waits=(\d+) pages=(\d+) pile=(\d+) threads=(\d+) endpoints=(\d+) nodes=(\d+) espaces=(\d+) cadres_ipc=(\d+) cadres_libres=Ok\((\d+)\)\r?\n',raw)]
    if not family or not {22,23}.issubset({p['service'] for p in family}):
        raise Refus('retrait natif du registre et du signataire non démontré')
    for p in family:
        matches=[v for v in values if v['pid']==p['pid']]
        if len(matches)!=1 or any(matches[0][k] for k in ('roots','handles','waits','pages','pile','threads')):
            raise Refus('restitution de la famille incomplète')
    if not values:raise Refus('mesures de restitution absentes')
    last=values[-1]
    survivors=[dict(zip(('after','pid','service','roots','handles','waits','pages','pile'),map(int,row))) for row in re.findall(
        rb'NAEXEC survivant apres_pid=(\d+) pid=(\d+) service=(\d+) roots=(\d+) handles=(\d+) waits=(\d+) pages=(\d+) pile=(\d+)\r?\n',raw)]
    survivors=[p for p in survivors if p['after']==last['pid']]
    baseline={(int(pid),int(service)) for pid,service in re.findall(rb'NAEXEC resident phase=consommation pid=(\d+) service=(\d+) pages=\d+ pile=\d+\r?\n',raw)}
    removed={p['pid'] for p in family}|{p['manager'] for p in family}
    expected={(pid,service) for pid,service in baseline if pid not in removed}
    if not baseline or {(p['pid'],p['service']) for p in survivors}!=expected:
        raise Refus('processus persistants différents de la référence avant refus')
    if len(survivors)!=last['espaces'] or sum(p['roots'] for p in survivors)!=last['endpoints'] or sum(p['handles'] for p in survivors)!=last['nodes']:
        raise Refus('ressources IPC ou espaces sans propriétaire persistant identifié')
    if any(p['roots']>4 for p in survivors):raise Refus('quota du processus persistant dépassé')
    return {'retrait_contraint':True,'famille':family,'restitutions':values,'persistants':survivors,'drainage_verifie':True}
