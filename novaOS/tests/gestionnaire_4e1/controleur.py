"""Erreur de collecte commune à deux domaines dans une copie de qualification de 4E.1."""
import argparse
import difflib
import io
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

NOVA=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(NOVA/"tests"))
from gestionnaire_4d.qualifier import BASE, ISA, PORT, REPO, sha, write

SOURCE="17bdcd7b0ff6ee112d1c34291ce9d2a7eee37d27"
PATCH="21207c5f41fc4ffdb61fdd477cc1a8b359ad2bd3122b72bc4194204b8350c52c"

def modifier(copie,relatif,avant,apres):
    p=copie/relatif
    source=p.read_text()
    if source.count(avant)!=1:
        raise ValueError("Point d’injection absent ou ambigu "+relatif)
    resultat=source.replace(avant,apres)
    p.write_text(resultat)
    return "".join(difflib.unified_diff(source.splitlines(True),resultat.splitlines(True),
                                        fromfile="a/"+relatif,tofile="b/"+relatif))

def construire(sortie):
    racine=Path(os.environ["NOVAOS_TMPDIR"]).resolve()
    cible=Path(os.environ["CARGO_TARGET_DIR"]).resolve()
    if racine not in cible.parents:
        raise ValueError("Répertoire de compilation privé requis")
    sortie.mkdir(parents=True,exist_ok=True)
    elf=sortie/"arm.elf"
    if elf.exists():
        m=json.loads((sortie/"construction.json").read_text())
        if m["base"]!=SOURCE or m["patch"]!=PATCH or sha(elf)!=m["elf"]["arm"] or sha(sortie/"injection.patch")!=m["patch"]:
            raise ValueError("Provenance du diagnostic")
        return elf,m["commit"]
    with tempfile.TemporaryDirectory(dir=racine,prefix="diagnostic-controleur-") as tmp:
        copie=Path(tmp)
        chemins=["novaOS/"+p for p in ("Cargo.toml","Cargo.lock",".cargo","kernel","abi","user","libs","tests/ethernet_v1")]
        chemins+=["novaFS/Cargo.toml","novaFS/Cargo.lock","novaFS/src","components/Cargo.toml","components/Cargo.lock",
                  "components/libraries","components/coreutils","components/shell","components/vi"]
        archive=subprocess.check_output(["git","archive",SOURCE,*chemins],cwd=REPO)
        with tarfile.open(fileobj=io.BytesIO(archive)) as t:
            t.extractall(copie,filter="data")
        # Le contrôleur modèle achève l'UNMAP. Seul le résultat du collecteur dans
        # cette copie est dégradé. Le chemin de mise en quarantaine reste celui de 4E.1.
        point="        let len = match q.completed() {"
        injection="""        let recu = q.completed();
        let recu = if recu.is_ok() && id.sid == NIC && f.op == Op::Unmap(0) {
            hal::log(format_args!("E41 diagnostic controleur recu_reel=true numero={} livraison=erreur", f.numero));
            Err(Error::Fault)
        } else { recu };
        let len = match recu {"""
        patch=modifier(copie,"novaOS/kernel/src/hardware/block/viommu_partage.rs",point,injection)
        point='            #[cfg(feature="gestionnaire-4d")]\n            if (matches!(option_env!("NOVAOS_4D_REFUS")'
        diagnostic="""            if trafic==Err(Error::Quarantine) {
                use super::net_arm::Preparation;
                let (numero,nic)=Preparation::attente_retrait(generation).ok_or(Error::State)?;
                let avant=crate::memory::pinned_dma_frames();
                let lecture=t.begin_read(table.volume);
                let flush=t.output(0,true);
                let recu=Preparation::reprendre_retrait(generation,numero);
                let admission=matches!(Preparation::ouvrir(),Err(Error::Quarantine));
                let retrait=t.close();
                let apres=crate::memory::pinned_dma_frames();
                hal::log(format_args!("E41 defaut commun generation={generation} numero={numero} nic={nic} avant={avant} lecture={lecture:?} flush={flush:?} recu={recu:?} admission_refusee={admission} retrait_disque={retrait:?} apres={apres}"));
                return Err(Error::Quarantine)
            }
"""
        patch+=modifier(copie,"novaOS/kernel/src/hardware/block/qualification.rs",point,diagnostic+point)
        fichier=sortie/"injection.patch";fichier.write_text(patch)
        if sha(fichier)!=PATCH:raise ValueError("Recette d’injection différente")
        env=dict(os.environ,NOVAOS_ETH3_CASE="reset-court",NOVAOS_4E1_CAS="controleur-commun",
                 NOVAOS_4D_REFUS="",CARGO_PROFILE_RELEASE_STRIP="none",
                 RUSTC=subprocess.check_output(["rustup","which","--toolchain","stable","rustc"],text=True).strip())
        cmd=["rustup","run","stable","cargo","build","--release","--locked","--offline","--target",ISA,
             "--features","gestionnaire-4e1"]
        with (sortie/"construction.log").open("w") as f:
            subprocess.run(cmd,cwd=copie/"novaOS",env=env,stdout=f,stderr=subprocess.STDOUT,check=True)
        shutil.copyfile(cible/ISA/"release/novaos",elf)
        identifiant=SOURCE+"+diagnostic."+sha(fichier)
        write(sortie/"construction.json",dict(commit=identifiant,base=SOURCE,patch=sha(fichier),elf={"arm":sha(elf)},
              commande=cmd,profil="copie de banc, erreur de collecte à la première clôture",rustc=subprocess.check_output([env["RUSTC"],"-V"],text=True).strip()))
    return elf,identifiant

def verifier(out,c,serie):
    import cycle_vie
    if c["arch"]!="arm" or c["gestionnaire_4e1"]!="controleur-commun" or (c["construction"]["base"],c["construction"]["patch"])!=(SOURCE,PATCH):
        raise ValueError("Profil du diagnostic")
    if (c["elf_relu_esp"]!=c["elf"] or not c["volume_relu_identique"] or not c["volume_p9"]["verification"]
            or c["erreur_pair"] or c["trames_par_sens"]!=1):
        raise ValueError("ELF, stockage ou pair du diagnostic")
    m=re.search(r"E41 diagnostic controleur recu_reel=true numero=(\d+) livraison=erreur",serie)
    if not m or not re.search(r"E41 defaut commun generation=(\d+) numero="+m[1]+r" nic=22 avant=38 lecture=Err\(Quarantine\) flush=Err\(Quarantine\) recu=Err\(Quarantine\) admission_refusee=true retrait_disque=Err\(Quarantine\) apres=38",serie):
        raise ValueError("Refus et cadres conservés par les propriétaires")
    marques=("DMA quarantaine","traductions_restantes=7","NET ARM bilan Err(Quarantine)",
             "gel=true compactage=false","D4 cycle avant=", "ipc=0 dma=38",
             "materiel=Ok(()) retrait=Ok(0) acteurs=[Exit(0), Exit(0), Exit(0)]")
    if not all(marque in serie for marque in marques) or not re.search(r"RAM phase=4d_fin_cycle .*?ipc=0 utilisateur=0 processus=0 ",serie):
        raise ValueError("Fermeture du port, gel ou collecte CPU")
    generations=[int(x) for x in re.findall(r"E3C admission generation=(\d+)",serie)]
    if len(generations)!=1 or not cycle_vie.verifier_reset("reset-court",c,serie,generations):
        raise ValueError("Reçus de reset du NIC")
    commandes=[]
    with (out/"qemu.trace").open() as trace:
        for line in trace:
            if line.startswith("pci_cfg_write virtio-blk-pci 00:06.0 @0x4 "):
                commandes.append(int(line.split()[-1],16))
    if not commandes or commandes[-1]&4 or not any(v&4 for v in commandes):
        raise ValueError("Activation et fermeture du bus mastering du disque")
    return dict(derniere_commande_pci_bloc=commandes[-1],origine="erreur de collecte injectée après un résultat matériel observé",dma_cadres=38,nic=22,stockage=7,controleur=9,
                iommu=cycle_vie.domaines_arm(out,set(range(1,21)),stockage_incertain=True))

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("sortie",type=Path)
    p.add_argument("--compiler",action="store_true")
    a=p.parse_args()
    racine=a.sortie.resolve()
    elf,ref=construire(racine/"image")
    if a.compiler:return
    fichiers=BASE/"archives-ethernet-phase3/reproduction-8bd3f9c"
    for i in range(1,4):
        dest=racine/str(i)
        if not dest.exists():
            cmd=[sys.executable,"-B",PORT,"arm","reset-court",elf,dest,BASE/"pilote-virtio-net",BASE/"archives-virtio-net",
                 fichiers/"limine-enroll",fichiers/"BOOTAA64.EFI","--limine-arm",fichiers/"limine-a72.EFI",
                 "--source-elf",ref,"--cycle-vie","--esp-verifiee","--gestionnaire-4e1","controleur-commun"]
            subprocess.run(cmd,cwd=REPO,check=True)
        c=json.loads((dest/"campagne.json").read_text())
        sys.path.insert(0,str(NOVA/"tests/ethernet_v1"))
        if not c["succes"] or c["elf"]!=sha(elf) or json.loads(json.dumps(verifier(dest,c,(dest/"serie.log").read_text())))!=c["cycle_vie"]:
            raise ValueError("Preuve incomplète "+str(dest))
        print(f"Contrôleur commun, démarrage {i} validé",flush=True)

if __name__=="__main__":main()
