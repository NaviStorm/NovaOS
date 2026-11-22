"""Lecture hors ligne des compteurs R1 invités, sans remplacer une recette QEMU."""
import argparse, hashlib, json, re
from pathlib import Path

def inspect(directory):
    directory=Path(directory)
    serial=directory/'invite/invite.serial'
    raw=serial.read_bytes();text=raw.decode(errors='replace')
    outcome=json.loads((directory/'r1-resultats.json').read_text())
    children=list(map(int,re.findall(r'NAEXEC enfant owner=5 child=(\d+)',text)))
    expected=len(outcome['commandes'])+len(outcome.get('refus_cibles',[]))+bool(outcome.get('reponse_partielle_perdue'))+2*bool(outcome.get('concurrence'))+bool(outcome.get('expiration'))+bool(outcome.get('nouvelle_session'))
    releases={}
    for m in re.finditer(r'NAEXEC restitue pid=(\d+) roots=(\d+) handles=(\d+) waits=(\d+) pages=(\d+) pile=(\d+) threads=(\d+)',text):
        pid,*counts=map(int,m.groups());releases[pid]=dict(zip(['roots','handles','waits','pages','pile','threads'],counts))
    peaks={}
    for m in re.finditer(r'pid=(\d+)[^\r\n]*?roots=(\d+) handles=(\d+)',text):
        pid,roots,handles=map(int,m.groups());a=peaks.setdefault(pid,[0,0]);a[0]=max(a[0],roots);a[1]=max(a[1],handles)
    rows=[]
    pattern=r'NAEXEC ressources phase=(\w+) owner=5 roots=(\d+) handles=(\d+) endpoints=(\d+) nodes=(\d+) waits=(\d+) processus=(\d+) threads=(\d+) joins=(\d+) cadres_ipc=(\d+) cadres_libres=Ok\((\d+)\)'
    names=['roots','handles','endpoints','nodes','waits','processus','threads','joins','cadres_ipc','cadres_libres']
    for m in re.finditer(pattern,text):rows.append({'phase':m[1],'position':m.start(),**dict(zip(names,map(int,m.groups()[1:])))})
    closed=[r for r in rows if r['phase']=='fermeture'];before=[r for r in rows if r['phase']=='avant_ticket']
    stable=['roots','handles','endpoints','nodes','processus','threads','joins']
    # Les deux fermetures de tickets du pipeline sont intermédiaires : le
    # contrôleur du pipe se ferme ensuite. Exiger sa restitution avant le
    # prochain lancement indépendant, et garder toutes les mesures partielles.
    pipe_start=text.find('nsh> admin-sonde --concurrent-writer |')
    pipe_end=text.find('NXPIPE producteur=',pipe_start) if pipe_start>=0 else -1
    reference=before[0] if before else {}
    same=lambda r:bool(reference) and all(r[k]==reference[k] for k in stable)
    intermediate=[r for r in closed if pipe_start<=r['position']<pipe_end]
    outside=[r for r in closed if r not in intermediate]
    following=next((r for r in before if r['position']>pipe_end),None)
    settled=bool(outside) and all(same(r) for r in outside) and (not outcome.get('concurrence') or (len(intermediate)==2 and following is not None and same(following)))
    resident={}
    for m in re.finditer(r'NAEXEC resident phase=(\w+) pid=(\d+) service=(\d+) pages=(\d+) pile=(\d+)',text):
        phase,pid,service,pages,stack=m.groups();resident.setdefault(pid,{'service':int(service),'pages':[],'pile':[]});resident[pid]['pages'].append(int(pages));resident[pid]['pile'].append(int(stack))
    for r in resident.values():r['pages']=sorted(set(r['pages']));r['pile']=sorted(set(r['pile']))
    result={'trace_sha256':hashlib.sha256(raw).hexdigest(),'enfants':children,'nombre_attendu':expected,
        'restitutions':{pid:releases.get(pid) for pid in children},'pics_racines_handles':peaks,
        'avant_pendant_apres':rows,'fermetures_intermediaires_pipe':intermediate,'reference':reference,'apres_drainage_pipe':following,'residents':resident,
        'tous_enfants_restitues':len(children)==expected and len(set(children))==expected and all(pid in releases and not any(releases[pid].values()) for pid in children),
        'retour_compteurs_fermeture':settled,'plafond':bool(peaks) and all(r[0]<=4 for r in peaks.values()),
        'limite':'Compteurs observés à des points explicites. Les attentes des services et la page IPC de collecte peuvent se drainer après la fermeture. Les pages résidentes du cache ne sont pas assimilées à des pages restituées.'}
    result['succes']=bool(outcome['succes'] and result['tous_enfants_restitues'] and settled and result['plafond'])
    return result

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('directory',type=Path);p.add_argument('output',type=Path);a=p.parse_args()
    result=inspect(a.directory);a.output.write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n')
    print(json.dumps({k:result[k] for k in ['succes','tous_enfants_restitues','retour_compteurs_fermeture','plafond']},ensure_ascii=False))
    raise SystemExit(0 if result['succes'] else 1)
