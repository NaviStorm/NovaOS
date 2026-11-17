"""Construit une extraction privée cli-test. Seul @probe reçoit des objets fictifs nommés."""
from laboratoire import *
import sys
arch=sys.argv[1]
target={"aarch64":"aarch64-unknown-none-softfloat","x86_64":"x86_64-unknown-none"}[arch]
lab=Path(sys.argv[2]).resolve()
lab.mkdir(exist_ok=False,parents=True)
with (lab/"source.tar").open("wb") as f:
    subprocess.run(["git","archive","HEAD","components","novaOS","novaFS"],cwd=ROOT,stdout=f,check=True)
subprocess.run(["tar","xf",lab/"source.tar","-C",lab],check=True)
def edit(name,old,new):
    p=lab/name;s=p.read_text();assert s.count(old)==1,name;p.write_text(s.replace(old,new))
# @probe est exclusivement une image de qualification, déjà admise avec QUALIFY.
# La variante conserve les droits de ce profil et le service P9 ordinaire.
edit("components/Cargo.toml",'path = "coreutils/probe.rs"','path = "diff/src/main.rs"')
edit("components/Cargo.toml","[dependencies]",'[features]\ndefault = ["temoin"]\ntemoin = []\n\n[dependencies]\nposix-regex = { version = "=0.1.4", features = ["no_std"] }')
arm='''
            Operand::Probe(case @ ("diff-corpus" | "diff-equal" | "diff-start" | "diff-end" | "diff-many" | "diff-tree")) => {
                fs.simple(w::STAGE,0)?;
                if *case == "diff-corpus" {fs.at(w::STAGE,0,b"/vecteurs",w::SELECT_FILE)?;}
                else if *case == "diff-tree" {
                    fs.at(w::STAGE,0,b"/treeA",w::SELECT_COPY_TREE)?;
                    fs.at(w::STAGE,0,b"/treeB",w::SELECT_COPY_TREE)?;
                } else {
                    fs.at(w::STAGE,0,b"/base",w::SELECT_FILE)?;
                    fs.at(w::STAGE,0,case.as_bytes(),w::SELECT_FILE)?;
                }
                (w::GROUP,0,alloc::string::String::new())
            }
'''
edit("components/shell/capabilities.rs",'            Operand::Probe("stat") => {',arm+'            Operand::Probe("stat") => {')
env={**ENV,"RUSTC":subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip()}
subprocess.run(["rustup","run","stable","cargo","generate-lockfile","--offline"],cwd=lab/"components",env=env,check=True)
subprocess.run(["rustup","run","stable","cargo","build","--locked","--offline","--release","--target",target],cwd=lab/"novaOS",env=env,check=True)
p=lab/"novaOS/target"/target/"release"
kernel=p/"novaos"
probe,=[x for x in p.glob("build/novaos-*/out/probe.elf") if x.read_bytes() in kernel.read_bytes()]
save(lab/"provenance.json",dict(head=subprocess.check_output(["git","rev-parse","HEAD"],cwd=ROOT,text=True).strip(),noyau=digest(kernel),probe=digest(probe)))
shutil.copy2(probe,lab/"probe.elf")
