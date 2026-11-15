"""Qualification ciblée des fenêtres de retrait et de la réattribution des cadres NVMe."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import sys

from materiel_pilote import read_config, sha256
from nvme_phase2 import ACK, Bench, application_order, eio_proof
from nvme_phase2_oracle import chronology, inspect, io_trace, unconfirmed_write, verify_transition
from nvme_qemu import dump
from vtd_edu import physical32, registers, require, table

CELLS = {"revoke-before":(2,512), "revoke-after":(4,4096), "timeout-flush":(2,4096),
         "reuse":(2,512),"eio-f2":(2,512)}
P2 = Path(__file__).resolve().parents[1]/"docs/validation/nvme-phase2-qemu.json"
P2_SHA = "712117f2192aeee7386bb966a746b7da3dfb8e86f90b4814b55137ff9a38fe3e"
BAIL = r"NVME P3 bail epoque=(\d+) pid=(\d+) slot=(\d+) iova=(0x[0-9a-f]+) gpa=(0x[0-9a-f]+)"


def leases(log):
    epochs = {}
    for epoch, owner, slot, iova, gpa in re.findall(BAIL,log):
        pages = epochs.setdefault(int(epoch),[])
        require(int(slot)==len(pages), "Indice de cadre absent, répété ou désordonné")
        pages.append({"iova":int(iova,16),"gpa":int(gpa,16),"pid":int(owner)})
    require(1 in epochs and len(epochs[1])==6 and all(len({p['pid'] for p in v})==1 for v in epochs.values()),
            "Titulaire ou nombre de cadres non établi par le superviseur")
    for pages in epochs.values():
        for key in ("iova","gpa"):
            require(len({p[key] for p in pages})==len(pages) and all(p[key]%4096==0 and p[key]>0 for p in pages),
                    "Adresse de page réutilisée dans le même bail ou non alignée")
    return epochs


def snapshot(channel, folder, log, when):
    """Relire les PTE et les pages physiques de la VM, sans écrire dans l'invité."""
    domain = registers(channel)
    allocations = leases(log)
    pages = {}
    def entries(addr):
        if addr not in pages:
            data = table(channel,addr,folder/f"p3-{when}-table-{addr:x}.bin")
            pages[addr] = struct.unpack("<512Q",data)
        return pages[addr]
    def pointer(word):
        return word & ~4095
    root = entries(domain["root"])
    require(root[0] & 4095 == 1 and not any(root[1:]), "Racine VT-d du profil Q35")
    context = entries(pointer(root[0]))
    require(not any(context[:96]) and not any(context[98:]), "Autre SID autorisé")
    mapped = {}
    if context[96]:
        require(context[96]&4095==1 and context[97]==0x102, "Contexte du SID 48")
        def walk(addr,shift,base):
            for index,entry in enumerate(entries(addr)):
                if entry:
                    iova = base | (index<<shift)
                    if shift==12:
                        mapped[iova] = (pointer(entry),entry&4095)
                    else:
                        require(entry&4095==3, "Droit inattendu sur un lien de tables VT-d")
                        walk(pointer(entry),shift-9,iova)
        walk(pointer(context[96]),39,0)
    else:
        require(not context[97], "Entrée contextuelle partiellement supprimée")
    command = read_config(channel,"x86_64",4,6,0) & 0xffff
    bar = re.search(r"NVME PCI .*?BAR0=(0x[0-9a-f]+)",log)
    expected_pci = 0x406 if when=='avant_retrait_lecture' else 0x402
    require(bar and command & 0x406==expected_pci, "État de BME, INTx ou BAR incorrect au point d'observation")
    base = int(bar[1],16)
    out = {"qi_iqh":domain["iqh"],"qi_iqt":domain["iqt"],"fsts":domain["fsts"],"commande_pci":command,
           "cc":physical32(channel,base+0x14),"csts":physical32(channel,base+0x1c),"pages":[]}
    expected = {}
    if when=='avant_retrait_lecture':
        expected = {p['iova']:(p['gpa'],r) for p,r in zip(allocations[2],(1,2,1,2,2))}
    elif 'NVME quarantaine ' in log:
        expected = {p['iova']:(p['gpa'],r) for p,r in zip(allocations[1],(1,2,1,2,2,1))}
    require(mapped==expected, "Les traductions DMA et les six/cinq cadres du bail ne correspondent pas")
    out['pte'] = sorted((iova,gpa,r) for iova,(gpa,r) in mapped.items())
    if when=='entre_incarnations':
        require(set(allocations)=={1} and 'NVME P3 nouvelle epoque=' not in log, "Seconde admission déjà commencée")
    if when=='avant_retrait_lecture':
        require('NVME P3 temoins intacts' not in log and out['cc']&1==0 and out['csts']&1==0,
                "Le contrôleur doit avoir terminé ses lectures avant le retrait des cinq PTE")
        require(len(allocations[2])==5 and len({p['gpa'] for v in allocations.values() for p in v})==11
                and len({p['iova'] for v in allocations.values() for p in v})==11,
                "Une page ou une IOVA de l'ancien propriétaire est réutilisée dans le nouveau bail")
        require(allocations[1][0]['pid']!=allocations[2][0]['pid'], "Le titulaire du second bail ne change pas")
    witness = when in ('entre_incarnations','avant_retrait_lecture','avant_liberation')
    if witness:
        require('NVME P3 temoins liberes cadres=6' not in log, "Rendez-vous avec les propriétaires manqué")
        marks = re.findall(r'NVME P3 cadre epoque=1 slot=(\d+) iova=(0x[0-9a-f]+) gpa=(0x[0-9a-f]+) motif=(0x[0-9a-f]+)',log)
        require(len(marks)==6, "La réallocation des six cadres n'est pas documentée")
    for slot,p in enumerate(allocations[1]):
        data = table(channel,p['gpa'],folder/f"p3-{when}-cadre-{slot}.bin")
        out['pages'].append(dict(p,sha256=hashlib.sha256(data).hexdigest()))
        if witness:
            require(tuple(int(v,0) for v in marks[slot])==(slot,p['iova'],p['gpa'],0xa0+slot)
                    and data==bytes([0xa0+slot])*4096, "Témoin physique du nouveau propriétaire modifié")
    return out


def verify(mode, boot, ops, log, seed, image, old):
    cpus, sector = CELLS[mode]
    require("NovaOS v0.19.0" in log, "Version du binaire invité")
    blk=boot['processus']['bloc']
    require(log.count(f'DEBUG pid={blk} NVME RW admissions statuts exacts verifies')==1,
            "Erreurs de capacité, époque, namespace, plage, taille et adresse dans le service bloc")
    result = {"profil":mode,"vcpus":cpus,"secteur":sector,
              "sha256_serie":boot['sha256_serie'],"sha256_trace":boot['sha256_trace'],"sha256_iso":boot['sha256_iso'],
              "ack_fichier":ACK in log,"write":boot['write'],"flush":boot['flush'],"retours":boot['processus']['retours']}
    if mode=='eio-f2':
        result['erreur_application']=eio_proof(mode,boot,ops,log)
        result['destination']='ancien ou nouveau'
        return result
    allocations = leases(log)
    frames = boot['cadres_physiques']
    final = frames['au_bilan']
    result.update(baux=allocations,observations_qmp=frames)
    if mode=='reuse':
        ids = [int(n) for n in re.findall(r'NVME P3 identify epoque=(\d+) cid=1\b',log)]
        counts=re.findall(r'NVME ressources initial=(\d+) final=(\d+) vcpus=(\d+)',log)
        require('NVME P3 temoins liberes cadres=6' in log and 'NVME P3 bilan temoins=0 dma=8' in log
                and set(allocations)=={1,2}
                and re.findall(r'NVME isolation tables_usager=4 cadres_interdits=(\d+) alias=0',log)==['14','19']
                and ids==[allocations[e][0]['pid'] for e in (1,2)]
                and counts==[('0','8',str(cpus))],
                "Réadmission ou restitution des témoins incomplète")
        result['cadres_dma_apres_restitution'] = 8
        second = [op for op in ops if op['epoque']==2]
        require(second and all(op['op']=='READ' and op.get('cqe')==0 for op in second),
                "READ seule ou CQE des lectures du second bail")
        first = [op for op in ops if op['epoque']==1]
        tx = chronology(first,old,sector)
        result['publication'] = application_order(tx,log)
        require(all(a['sha256']==b['sha256']==c['sha256'] for a,b,c in
                    zip(frames['entre_incarnations']['pages'],frames['avant_retrait_lecture']['pages'],frames['avant_liberation']['pages'])),
                "Contenu des six cadres réattribués instable pendant le second bail")
        result['cid_io_reutilises'] = sorted({op['cid'] for op in first}&{op['cid'] for op in second})
        require(result['cid_io_reutilises'] and 'NVME P9 oracle nouveau' in log,
                "Réemploi du CID ou intégrité du fichier non établis")
        result['destination']='nouveau'
    else:
        writes = [o for o in ops if o['op']=='WRITE']
        require(ACK not in log and all(o.get('cqe')==0 for o in ops), "Commande manquante/en erreur ou faux succès")
        if mode in ('revoke-before','revoke-after'):
            point,tag,num_writes=('avant',23,0) if mode=='revoke-before' else ('apres',19,1)
            marker=re.search(rf'NVME P3 retrait point={point} cpu=(\d+) ticket=(\d+) operation={tag} statut=5 soumises=(\d+) terminees=(\d+)',log)
            require(marker and 0 < int(marker[1]) < cpus and int(marker[2])>0 and marker[3]==marker[4]
                    and len(writes)==num_writes and boot['flush']==1
                    and boot['processus']['retours'][str(boot['processus']['application'])]==5,
                    "Frontière de retrait, refus du ticket ou statut du client")
            result['refus']={"operation":tag,"statut":5,"cpu":int(marker[1]),"ticket":int(marker[2])}
        elif mode=='timeout-flush':
            f1=ops[-1]
            require(f1==boot['cqe_tardive_apres_bilan'] and f1['op']=='FLUSH' and len(writes)>0
                    and re.search(rf'NVME RW erreur commande=Flush .*cid={f1["cid"]} statut_nvme=None erreur=Timeout',log)
                    and 'expiration=true quarantaine=true retrait_confirme=false' in log,
                    "Échéance du FLUSH, quarantaine et CQE ultérieure du même CID")
            require(boot['processus']['retours'][str(boot['processus']['application'])]==8,
                    "Le délai du FLUSH doit atteindre le client")
            require('NVME P9 statut bloc=08' in log and frames['avant_cqe_tardive']['pte']==final['pte'],
                    "Statut du service P9 ou stabilité du domaine retenu")
            result['cqe_tardive']=f1
            result['cadres_modifies_par_cqe']=[i for i,(p,q) in enumerate(zip(frames['avant_cqe_tardive']['pages'],final['pages'])) if p['sha256']!=q['sha256']]
        if writes:
            result['effet_sur_raw']=unconfirmed_write(seed,image,ops,sector)
        else:
            require(sha256(seed)==sha256(image), "Le RAW a changé sans soumission de WRITE")
        result['destination']='ancien'
    if mode!='timeout-flush':
        require(not final['pte'] and final['fsts']==0 and final['qi_iqt']==final['qi_iqh']
                and not(final['cc']&1 or final['csts']&1) and 'WAIT=confirme pages_restituees=6' in log,
                "Arrêt contrôleur ou retrait VT-d avant restitution non confirmé")
    result['qi_et_wait']=boot['domaine'].get('qi_et_wait')
    return result


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--elf',required=True,type=lambda s:Path(s).resolve(strict=True))
    p.add_argument('--lecteur',required=True,type=lambda s:Path(s).resolve(strict=True))
    p.add_argument('--sortie',required=True,type=lambda s:Path(s).resolve(),help='Répertoire neuf pour les preuves')
    p.add_argument('--point',action='append',choices=CELLS)
    p.add_argument('--delai',type=int,default=120)
    args=p.parse_args()
    require(sha256(P2)==P2_SHA,'La référence de qualification de phase 2 a changé')
    b=Bench(args)
    rows=[]
    for mode in dict.fromkeys(args.point or CELLS):
        print(mode,flush=True)
        cpus,sector=CELLS[mode]
        seed,old=b.seed(sector)
        folder=b.dir/mode
        folder.mkdir()
        img=folder/'disque.raw'
        shutil.copyfile(seed,img)
        boot,ops,log=b.vm(mode,cpus,sector,img,folder/'operation',
                          frame_probe=None if mode=='eio-f2' else snapshot)
        row=verify(mode,boot,ops,log,seed,img,old)
        checked=inspect(b.reader,img,sector)
        allowed=('ancien','nouveau') if mode=='eio-f2' else (row['destination'],)
        verify_transition(old,checked,allowed)
        row['destination']=checked['etat']
        cold,_,rlog=b.vm('recover',cpus,sector,img,folder/'remontage')
        require(cold['sha256_raw_avant']==cold['sha256_raw_apres']==boot['sha256_raw_apres']
                and f'NVME P9 oracle {checked["etat"]}' in rlog and not(cold['write'] or cold['flush']),
                'Remontage indépendant sur une nouvelle VM en lecture seule')
        row['relecture']={k:checked[k] for k in ('generation','sha256_fichier','sha256_garde','ancres')}
        row['relecture'].update(raw_sha256=cold['sha256_raw_apres'], trace_sha256=cold['sha256_trace'],serie_sha256=cold['sha256_serie'])
        rows.append(row)
        dump(b.dir/'phase3.json', {k:b.report[k] for k in ('sha256_elf','sha256_lecteur','firmware','qemu','geometrie')} |
             {'phase2_sha256':P2_SHA, 'cellules':rows})


def report():
    """Reconstituer un seul rapport, y compris si les profils ont été lancés séparément."""
    p=argparse.ArgumentParser(description=report.__doc__)
    p.add_argument('lots',type=Path,nargs='+')
    p.add_argument('--lecteur',required=True,type=lambda s:Path(s).resolve(strict=True))
    p.add_argument('--rapport',required=True,type=Path)
    args=p.parse_args(sys.argv[2:])
    result=None
    profiles={}
    for lot in args.lots:
        doc=json.loads((lot/'phase3.json').read_bytes())
        meta={k:v for k,v in doc.items() if k!='cellules'}
        require(result in (None,meta) and sha256(lot/'novaos.elf')==meta['sha256_elf']
                and sha256(args.lecteur)==meta['sha256_lecteur'] and meta['phase2_sha256']==sha256(P2)==P2_SHA,
                "Les preuves doivent couvrir exactement les mêmes binaires et la même référence")
        result=meta
        for saved in doc['cellules']:
            name=saved['profil']
            require(name in CELLS and name not in profiles,"Profil inattendu ou dupliqué")
            base=lot/f"initial-{saved['secteur']}"/'initial.raw'
            old=inspect(args.lecteur,base,saved['secteur'])
            folder=lot/name
            disk=folder/'disque.raw'
            run=folder/'operation'
            info=json.loads((run/'bilan.json').read_bytes())
            iso=lot/f"nvme-rw-{'update' if name=='eio-f2' else name}.iso"
            require(sha256(base)==info['sha256_raw_avant'] and sha256(run/'serie.log')==saved['sha256_serie']
                    and sha256(iso)==saved['sha256_iso'] and sha256(run/'trace.log')==saved['sha256_trace'],
                    "Preuve d'exécution altérée")
            ops=io_trace(run/'trace.log',saved['secteur'],2 if name=='reuse' else 1)
            rebased=verify(name,info,ops,(run/'serie.log').read_text(errors='replace'),base,disk,old)
            for key,value in rebased.items():
                if key!='destination':
                    require(saved[key]==json.loads(json.dumps(value)),f"Résultat {name}/{key} incohérent")
            for sample,group in info.get('cadres_physiques',{}).items():
                tag={'au_bilan':'final','avant_cqe_tardive':'avant'}.get(sample,sample)
                for i,page in enumerate(group['pages']):
                    require(sha256(run/f'p3-{tag}-cadre-{i}.bin')==page['sha256'], 'Cadre témoin altéré')
            latest=inspect(args.lecteur,disk,saved['secteur'])
            verify_transition(old,latest,('ancien','nouveau') if name=='eio-f2' else (rebased['destination'],))
            cold=folder/'remontage'
            reopen=json.loads((cold/'bilan.json').read_bytes())
            proof=saved['relecture']
            require(sha256(disk)==proof['raw_sha256']==reopen['sha256_raw_avant']==reopen['sha256_raw_apres']
                    and sha256(lot/'nvme-rw-recover.iso')==reopen['sha256_iso']
                    and not (reopen['write'] or reopen['flush']) and proof['generation']==latest['generation']
                    and saved['destination']==latest['etat'] and proof['sha256_fichier']==latest['sha256_fichier']
                    and proof['sha256_garde']==latest['sha256_garde'] and proof['ancres']==json.loads(json.dumps(latest['ancres']))
                    and sha256(cold/'trace.log')==proof['trace_sha256'] and sha256(cold/'serie.log')==proof['serie_sha256']
                    and f"NVME P9 oracle {latest['etat']}" in (cold/'serie.log').read_text(errors='replace'),
                    "Remontage et contrelecture du RAW incompatibles")
            profiles[name]=saved
    require(profiles.keys()==CELLS.keys(),"Couverture incomplète")
    with args.rapport.open('x') as out:
        json.dump(result | {'cellules':[profiles[name] for name in CELLS]},out,ensure_ascii=False,indent=2)
        out.write('\n')


if __name__=='__main__':
    if sys.argv[1:2]==['rapport']:
        report()
    else:
        main()
