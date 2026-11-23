from pathlib import Path
import json,re,hashlib,collections,datetime
b=Path('/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees');n=b/'archives-novaauth-administration-0240/integration-selective-49739dd'
rows=[]
for c in sorted(n.glob('r2-commandes-*-coordination.json')):
 co=json.loads(c.read_text());name=c.name.removesuffix('-coordination.json');r=n/name
 if not (r/'invite/resultat.json').exists():continue
 a=json.loads((r/'invite/resultat.json').read_text());s=json.loads((r/'commandes-resultats.json').read_text())
 raw=(r/'invite/invite.serial').read_bytes();mem={}
 for pid,service,pages,stack in re.findall(rb'NAEXEC (?:survivant apres_pid=\d+|resident phase=\w+) pid=(\d+) service=(\d+).*? pages=(\d+) pile=(\d+)',raw):
  p=int(pid);v=mem.setdefault(p,{'service':int(service),'pages_max_observees':0,'pile_max_observee':0});v['pages_max_observees']=max(v['pages_max_observees'],int(pages));v['pile_max_observee']=max(v['pile_max_observee'],int(stack))
 freed=[int(x)for x in re.findall(rb'cadres_libres=Ok\((\d+)\)',raw)]
 terminal=re.findall(rb'endpoints=(\d+) nodes=(\d+) espaces=(\d+) cadres_ipc=(\d+)',raw)
 row={'essai':name,'profil':f"{a['architecture']}/P9-{a['secteur']}",'source_invitee':co['commit_de_reference'],'arbre_invite':co.get('arbre_noyau_services'),'commit_banc_annonce':co.get('commit_banc'),'banc_sha256':co.get('banc_sha256'),'collecteur_sha256':co.get('collecteur_sha256'),'succes':a['succes'],'statut_banc':co.get('code'),'erreur':a.get('erreur_invite'),'commandes':s['commandes'],'retrait':s.get('retrait_selectif'),'rpc':s.get('rpc_selectifs'),'ordre':s.get('ordre_durable'),'preuve':str((r/'invite/resultat.json').relative_to(n)),'sha256':hashlib.sha256((r/'invite/resultat.json').read_bytes()).hexdigest(),'acquisition':co.get('acquisition'),'liberation':co.get('liberation'),'attente_s':co.get('attente_secondes'),'execution_s':co.get('execution_secondes'),'verrou_libere':co.get('verrou_libere'),'conflits_evites':co.get('conflits_evites'),'ressources_registre':a.get('ressources_registre'),'ressources_signataire':a.get('ressources'),'racines_max_par_pid':a.get('pics_racines_par_pid'),'cinquieme_racine_refusee':a.get('cinquieme_racine_refusee'),'connexions_banc':a.get('connexions_banc'),'limite_connexions':a.get('limite_connexions_banc'),'tls_monotone_invite':a.get('echanges_tls_horodates'),'memoire_observee_par_pid':mem,'cadres_libres_min_observes':min(freed)if freed else None,'cadres_libres_fin_observee':freed[-1]if freed else None,'drainage_final':list(map(int,terminal[-1]))if terminal else None}
 if name.endswith('retrait-selectif-vide-02'):row['source_invitee_corrigee']='1f69c2bcb4ca0c719607d838f49ed3921f574164';row['erreur_tracabilite']='voir correction-tracabilite-essai-02.json'
 rows.append(row)
profiles={}
for row in rows:
 p=profiles.setdefault(row['profil'],{'essais':0,'echecs':0,'succes':0});p['essais']+=1;p['echecs']+=not row['succes'];p['succes']+=row['succes']
summary={'date':'2026-11-23','commit_code':json.loads((n/'commit-code.json').read_text())['commit'],'qualification_complete':False,'runs':rows,'par_profil':profiles,'temps_attente_s':sum(r['attente_s']or 0 for r in rows),'temps_execution_s':sum(r['execution_s']or 0 for r in rows),'Q1_Q4':{p:{'Q1':'NON_EXECUTE_INVITE','Q2':'NON_EXECUTE_INVITE','Q3':'NON_EXECUTE_INVITE','Q4':'PARTIEL_SANS_DELEGATION_G_OUVERTE'}for p in profiles},'limite_memoire':'Maxima des échantillons disponibles, pas une mesure continue du pic physique ni une levée de réserve AArch64 301 cadres.'}
(n/'matrice-integration.json').write_text(json.dumps(summary,indent=2,ensure_ascii=False)+'\n');print(json.dumps({'profils':profiles,'attente_s':summary['temps_attente_s'],'execution_s':summary['temps_execution_s']},ensure_ascii=False))
