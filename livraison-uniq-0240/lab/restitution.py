"""Comparer la collecte à une session témoin qui lance uniquement pwd."""
import argparse,json,os,re,shutil,sys
from pathlib import Path
from bench import ROOT
sys.path.insert(0,str(ROOT.parent/"novaOS/tests"))
from cli import Terminal
from journalisation import arret
from pipe_head import drained

if __name__=="__main__":
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch");p.add_argument("sector",type=int);p.add_argument("campagne",type=Path);p.add_argument("out",type=Path)
    a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
    os.environ.update(NOVA_RAM="256",NOVA_VCPUS="2")
    image=out/"temoin.raw";shutil.copyfile(a.campagne/"session.raw",image)
    serial=out/"temoin.serial"
    with Terminal(a.arch,a.sector,image,"uniq-temoin",test=True,journal=serial) as vm:
        vm.until(b"nsh> ");vm.line("pwd",has=["[statut sortie 0]"]);arret(vm,2)
    def resources(log):
        line=re.search(rb"^RAM phase=bloc_collecte .*",log.read_bytes(),re.M)
        return {k.decode():int(v) for k,v in re.findall(rb"\b(ipc|utilisateur|processus|dma)=(\d+)",line[0])}
    base=resources(serial);observed=resources(a.campagne/"droits.serial")
    (out/"restitution.json").write_text(json.dumps(dict(temoin=base,campagne=observed,ok=base==observed),indent=1)+"\n")
    assert len(base)==4 and base==observed
