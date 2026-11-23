from pathlib import Path
import os,subprocess,json,hashlib,shutil,time,sys
arch=sys.argv[1];assert arch in ("aarch64","x86_64")
target_name="aarch64-unknown-none-softfloat" if arch=="aarch64" else "x86_64-unknown-none"
b=Path('/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees');t=b/'tmp/r2-integration-49739dd';d=b/'artefacts-prives-administration-0240-r2/integration-49739dd-08';d.mkdir(mode=0o700,exist_ok=True);n=b/'archives-novaauth-administration-0240/integration-selective-49739dd'
target=b/'tmp/r2-selectif-effecteurs-3036827/target-invite';kernel=target/target_name/'release/novaos';reg=list((target/target_name/'release/build').glob('novaos-*/out/service-target/'+target_name+'/release/registre-incarnation'));assert len(reg)==1
shutil.copyfile(kernel,d/(arch+'-kernel.elf'));shutil.copyfile(reg[0],d/(arch+'-registre.elf'))
e={**os.environ,'TMPDIR':str(b/'tmp')+'/','NOVAOS_TMPDIR':str(b/'tmp'),'NOVA_ARCH':arch,'NOVA_KERNEL_ELF':str(d/(arch+'-kernel.elf')),'CARGO_TARGET_DIR':str(target)}
start=time.monotonic();cmd=['sh','scripts/iso.sh','novaauth-sessions',str(d/(arch+'.iso'))]
with (n/('iso-08-'+arch+'.log')).open('x') as f:r=subprocess.run(cmd,cwd=t/'export-08/novaOS',env=e,stdout=f,stderr=subprocess.STDOUT)
if r.returncode:raise SystemExit(r.returncode)
extract=t/('iso-extract-08-'+arch);extract.mkdir()
with (n/('iso-08-validation-'+arch+'.log')).open('x') as f:
 subprocess.run(['xorriso','-osirrox','on','-indev',str(d/(arch+'.iso')),'-extract','/limine.conf',str(extract/'limine.conf'),'-extract','/boot/novaos.elf',str(extract/'kernel.elf')],check=True,stdout=f,stderr=subprocess.STDOUT,env=e)
assert 'cmdline: novaauth-sessions' in (extract/'limine.conf').read_text()
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();assert sha(extract/'kernel.elf')==sha(d/(arch+'-kernel.elf'))
r={'source':'95defcc22404b35ffd9b06dbcbbc33af30538bfe','profil':'membership_r2+administration_last_admin','commande':cmd,'duree_hote_s':time.monotonic()-start,'artefacts':{str(p):sha(p) for p in d.iterdir() if p.name.startswith(arch)},'profil_ISO_verifie':True,'noyau_ISO_identique':True};(n/('iso-08-'+arch+'.json')).write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r))
