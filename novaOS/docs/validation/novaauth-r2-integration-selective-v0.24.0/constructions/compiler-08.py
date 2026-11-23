from pathlib import Path
import os,subprocess,time,json,sys
b=Path('/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees');v=b/'artefacts-prives-administration-0240-r1'
d=b/'tmp/r2-integration-49739dd/export-08';logs=b/'archives-novaauth-administration-0240/integration-selective-49739dd'
e=os.environ.copy();e.update(RUSTC='/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc',CARGO_TARGET_DIR=str(b/'tmp/r2-selectif-effecteurs-3036827/target-invite'),NOVA_DISPLAY_TRACE='1',TMPDIR=str(b/'tmp')+'/',NOVAOS_TMPDIR=str(b/'tmp'))
c=b/'artefacts-prives-administration-0240-r2/certificats-fonctions-05'
for key,path in {'NOVA_QEMU_SERVER_SEED':c/'serveur','NOVA_QEMU_SERVER_PUB':v/'serveur.pub','NOVA_FIXTURE_CA':c/'ca.der','NOVA_AUTHORITY_CERT':c/'autorite.der','NOVA_AUTHORITY_PUB':c/'autorite.pub','NOVA_GUEST_CERT':c/'serveur.der','NOVA_DECISION_PUB':v/'decision.pub','NOVA_LAB_ALICE':v/'alice','NOVA_LAB_BOB':v/'bob','NOVA_PACKAGE_PUBLIC':v/'distribution.pub','NOVA_VFS_OBJECTS':b/'preuves-recuperation-v2-cf3d218/vfs-volume-512/dossiers.bin'}.items():e[key]=str(path)
arch,label=sys.argv[1:];target='aarch64-unknown-none-softfloat' if arch=='aarch64' else 'x86_64-unknown-none'
cmd=['/Users/thierry/.rustup/toolchains/stable-aarch64-apple-darwin/bin/cargo','build','--release','--locked','--offline','--features','novaauth-sessions-lab','--target',target];start=time.monotonic()
with (logs/(label+'.log')).open('x') as f:r=subprocess.run(cmd,cwd=d/'novaOS',env=e,stdout=f,stderr=subprocess.STDOUT)
result={'architecture':arch,'commande':cmd,'statut':r.returncode,'duree_hote_s':time.monotonic()-start,'source':'95defcc22404b35ffd9b06dbcbbc33af30538bfe export membership_r2+administration_last_admin, pas QEMU'};(logs/(label+'.json')).write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result));sys.exit(r.returncode)
