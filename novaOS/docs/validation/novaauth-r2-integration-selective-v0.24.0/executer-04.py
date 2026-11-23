from pathlib import Path
import datetime,json,os,signal,subprocess,sys,time,uuid,select
b=Path('/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees');w=b/'novaos-novaauth-administration-0240-phase1'
p=b/'archives-novaauth-administration-0240/integration-selective-49739dd';p.mkdir(exist_ok=True); d=b/'artefacts-prives-administration-0240-r2/integration-49739dd-04';v=b/'artefacts-prives-administration-0240-r1';lock=b/'.qualification-utilitaires-exclusive.lock'
arch,sector,lot,attempt=sys.argv[1:5]
assert arch in ('x86_64','aarch64') and sector in ('512','4096')
selected=sys.argv[5:];name='r2-commandes-'+arch+'-'+sector+'-'+lot+'-'+attempt
record_path=p/(name+'-coordination.json');assert not record_path.exists()
def utc():return datetime.datetime.now(datetime.timezone.utc).isoformat()
record={'debut_attente':utc(),'debut_attente_ns':time.monotonic_ns(),'architecture':arch,'secteur':int(sector),'conflits_evites':[]}
def save():record_path.write_text(json.dumps(record,indent=2)+'\n')
owner={'pid':os.getpid(),'token':str(uuid.uuid4()),'worktree':str(w),'utilitaire':'NovaAuth','action':'raccord duplex R2 '+name}
owned=False;child=None;code=1
def stop(sig,frame):raise KeyboardInterrupt
signal.signal(signal.SIGTERM,stop);signal.signal(signal.SIGINT,stop)
save()
try:
 while not owned:
  try:lock.mkdir();owned=True
  except FileExistsError:
   try:other={f.name:f.read_text() for f in lock.iterdir() if f.is_file()}
   except FileNotFoundError:continue
   if not record['conflits_evites'] or record['conflits_evites'][-1]!=other:record['conflits_evites'].append(other);save();print('Attente verrou commun',flush=True)
   time.sleep(10)
 (lock/'owner.json').write_text(json.dumps(owner));record.update(acquisition=utc(),acquisition_ns=time.monotonic_ns());record['attente_secondes']=(record['acquisition_ns']-record['debut_attente_ns'])/1e9;save()
 active=[r for r in subprocess.check_output(['ps','-axo','pid=,ppid=,comm='],text=True).splitlines() if 'qemu-system-' in r or 'hyperfine' in r]
 if active:raise RuntimeError('Campagne incompatible active : '+repr(active))
 env={**os.environ,'NOVAOS_TMPDIR':str(b/'tmp'),'TMPDIR':str(b/'tmp')+'/',
 'NOVA_ADAPTATEUR':str(b/'artefacts-prives-administration-0240-r2/finalisation-c80bdcb/adaptateur-page512')}
 # Reprendre le transport natif déjà qualifié, jamais un relais Python non borné.
 if not Path(env['NOVA_ADAPTATEUR']).is_file():raise RuntimeError('Adaptateur natif exact à localiser')
 cmd=[sys.executable,'-B',str(w/'novaOS/user/novaauth/autorite_qemu/administration_commandes_qemu.py'),arch,sector,'--kernel',str(d/(arch+'-kernel.elf')),
 '--iso',str(d/(arch+'.iso')),'--registre',str(d/(arch+'-registre.elf')),
 '--genese',str(b/'artefacts-prives-administration-0240-r2/fonctions-08'/('genese-'+arch+'-'+sector)/'identites.p9'),'--triplet',str(b/'artefacts-prives-administration-0240-r2/fonctions-08'/('genese-'+arch+'-'+sector)/'triplet.bin'),
 '--cles',str(b/'artefacts-prives-administration-0240-r2/certificats-fonctions-05'),'--decision',str(v/'decision'),
 '--detenteur',str(b/'artefacts-prives-raccord-exec-42128c2/outils-recuperation/detenteur'),
 '--capture',str(b/'artefacts-prives-raccord-exec-42128c2/outils-recuperation/capture'),
 '--sortie',str(p/name),'--secrets',str(b/'artefacts-prives-administration-0240-r2/cinq-fonctions-82349ed'/name)]
 record['commit_de_reference']='8140dbc6dfedc4620205ba77ed584f12d224e787'
 record['arbre_noyau_services']='8de7698f841cca565ece3a61a9b74f24395bb5bb'
 record['commit_banc']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=w,text=True).strip()
 record['banc_sha256']=__import__('hashlib').sha256((w/'novaOS/user/novaauth/autorite_qemu/administration_commandes_qemu.py').read_bytes()).hexdigest()
 import hashlib
 record['artefacts_sha256']={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [d/(arch+'.iso'),d/(arch+'-kernel.elf'),d/(arch+'-registre.elf'),b/'artefacts-prives-administration-0240-r2/fonctions-08'/('genese-'+arch+'-'+sector)/'identites.p9',b/'artefacts-prives-administration-0240-r2/fonctions-08'/('genese-'+arch+'-'+sector)/'autorite.raw']}
 build=json.loads((p/'iso-04-x86.json').read_text())
 if build['source']!=record['commit_de_reference']:raise RuntimeError('Source ISO différente du candidat')
 for artifact,digest in build['artefacts'].items():
  if record['artefacts_sha256'].get(artifact)!=digest:raise RuntimeError('Artefact non qualifié : '+artifact)
 record['liaison_source_artefacts_verifiee']=True
 if lot=='groupe':
  for option,filename in [('--genese','identites.p9'),('--triplet','triplet.bin')]:
   cmd[cmd.index(option)+1]=str(b/'artefacts-prives-administration-0240-r2/fonctions-05'/('genese-recu-'+arch+'-'+sector)/filename)
 cmd += ['--lot',lot,'--bob-minimal',*selected]
 record['adaptateur_sha256']=hashlib.sha256(Path(env['NOVA_ADAPTATEUR']).read_bytes()).hexdigest()
 record['adaptateur_option']='--cfg r2_page_512'
 record.update(commande=cmd,execution_ns=time.monotonic_ns());save();print('Verrou acquis ; essai',name,flush=True)
 with (p/(name+'.log')).open('wb') as log:
  child=subprocess.Popen(cmd,cwd=w,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
  code=child.wait(timeout=360)
 record.update(code=code,execution_secondes=(time.monotonic_ns()-record['execution_ns'])/1e9)
finally:
 if child is not None and child.poll() is None:
  os.killpg(child.pid,signal.SIGINT)
  try:child.wait(timeout=15)
  except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGTERM);child.wait(timeout=10)
 if owned:
  record['liberation']=utc()
  if (lock/'owner.json').exists() and json.loads((lock/'owner.json').read_text())==owner:
   (lock/'owner.json').unlink();lock.rmdir();record['verrou_libere']=True
  else:record['verrou_libere']=False
 save();print('Fin essai',name,'code',code,'verrou libéré',record.get('verrou_libere'),flush=True)
raise SystemExit(code)
