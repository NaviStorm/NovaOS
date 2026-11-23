from pathlib import Path
import subprocess,os,json,runpy,sys
w=Path('/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/novaos-novaauth-administration-0240-phase1');b=w.parent;t=b/'tmp/r2-integration-49739dd';e=os.environ.copy();e['GIT_INDEX_FILE']=str(t/'index-compilation');e['TMPDIR']=str(b/'tmp')+'/'
paths=subprocess.check_output(['git','diff','--name-only'],cwd=w,text=True).splitlines()+subprocess.check_output(['git','ls-files','--others','--exclude-standard'],cwd=w,text=True).splitlines()
assert all(x.startswith('novaOS/user/novaauth/') for x in paths)
subprocess.run(['git','read-tree','HEAD'],cwd=w,env=e,check=True);subprocess.run(['git','add','--',*paths],cwd=w,env=e,check=True)
tree=subprocess.check_output(['git','write-tree'],cwd=w,env=e,text=True).strip()
ref=subprocess.check_output(['git','commit-tree',tree,'-p','HEAD','-m','NovaAuth v0.24.0 — instantané local de compilation R2'],cwd=w,env=e,text=True).strip()
(t/'candidate-08.json').write_text(json.dumps({'source':ref,'tree':tree,'publication':False,'paths':paths},indent=2)+'\n')
sys.path.insert(0,str(w/'novaOS/user/novaauth/service'));runpy.run_path(str(w/'novaOS/user/novaauth/service/profile.py'))['prepare'](t/'export-08',source_ref=ref,membership_r2=True,administration_last_admin=True)
print(ref,tree)
