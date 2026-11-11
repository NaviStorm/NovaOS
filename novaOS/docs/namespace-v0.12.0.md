# Espace de noms natif — audit de v0.12.0

**Décision de livraison.** Arrêt au contrôle d’intégrité de l’espace de noms sur la base `24609a52e64a5626527827babc912ba7731d70de`. Un renommage valide fait disparaître l’accès à un fichier par le moteur public, alors que son entrée reste énumérable et que le volume passe la vérification intégrale. Un renommage inverse restitue le même objet et les mêmes octets. Le choix à trancher porte sur l’accès aux arbres déjà publiés par NovaFS. NovaOS reste en v0.11.0 (MIT) et NovaFS en v0.9.12 (MPL-2.0).

## Témoin sur un volume autonome

Le fichier se trouve sous cinq composants UTF-8 ASCII. Son chemin commence par `/a`, puis contient trois noms de 250 octets et un nom de fichier de 64 octets. Le moteur n’utilise ni lien ni alias dans cet exemple. Le laboratoire suit le même fichier sur trois générations.

| Génération | Opération confirmée | Observation après montage |
|---|---|---|
| 7 | Écriture des quatre octets `4e 6f 76 61` dans l’objet 6 | Le chemin de 820 octets permet de lire `Nova`. |
| 8 | Renommage de `/a` à la racine, avec un nom de 255 octets | Le parent est accessible par un chemin de 1 009 octets. Sa liste contient le fichier. L’ancien chemin renvoie `NotFound`. Le nouveau chemin de 1 074 octets renvoie `Invalid("chemin")` pour `stat` et pour la lecture. Même sans le premier séparateur, il compte 1 073 octets. |
| 9 | Renommage inverse, dans une transaction distincte | Une nouvelle ouverture retrouve l’objet 6 et les quatre octets exacts. |

La limite de 1 024 octets est à l’entrée de `components`. Les opérations fichier publiques reconstruisent leur parcours depuis la racine. `rename` vérifie ses deux arguments, le cycle et la profondeur finale. Il examine la hauteur du sous-arbre si la destination est plus profonde. Le changement de longueur du préfixe à profondeur constante échappe à ce contrôle. Le vérificateur compare les index directs et inverses et remonte les parents jusqu’à la racine. Ces invariants restent vrais en G8. Le volume conserve ses données et ses deux ancres. C’est l’accessibilité logique d’un fichier publié qui régresse. [Résolution et renommage](../../novaFS/src/fs.rs) · [Lecture par chemin](../../novaFS/src/fs/data.rs) · [Vérification du catalogue](../../novaFS/src/fs/check.rs)

Un contrôle limité aux nouvelles requêtes NovaOS pourrait empêcher la création du témoin côté invité. Il laisserait le service incapable de lire le fichier sur un volume G8 importé de macOS, y compris si le client fournit deux chemins relatifs courts. Aujourd’hui, le moteur public part de la racine même en l’absence d’un séparateur initial. Fragmenter davantage les trames IPC ne change pas ce point.

## Résultats exécutés

| Hôte macOS | Secteurs | Contrôle à 1 024 octets | Cas à 1 026 et 1 074 octets | Contrôle après renommage inverse |
|---|---|---|---|---|
| ARM64 natif | 512 et 4 096 | Objet 6, lecture exacte | Nom listé, `stat` absolu et lecture même sans slash initial refusés | Objet 6, lecture exacte |
| x86_64 sous Rosetta | 512 et 4 096 | Objet 6, lecture exacte | Mêmes résultats | Objet 6, lecture exacte |

À 1 025 octets, l’entrée absolue est refusée mais la forme sans séparateur initial, longue de 1 024 octets, permet encore de lire le fichier depuis la racine. Ce cas frontière ne résout pas G8. Les seize combinaisons utilisent un fichier image temporaire, un démontage et un nouveau montage RO. Le moteur relit les deux générations. Un contrôle distinct par la CLI de production a extrait `Nova` depuis le slot de G7 et a reproduit le refus sur le slot de G8 dans les quatre configurations.

| Invité QEMU | Secteur | Écritures disque / FLUSH terminés à chaque arrêt G8 et G9 | Nouvelle VM RO après G8 | Nouvelle VM RO après G9 |
|---|---:|---:|---|---|
| x86_64 | 512 | 40 / 3 | Entrée listée, lecture refusée | Objet 6, `Nova` |
| x86_64 | 4 096 | 5 / 3 | Identique | Identique |
| AArch64 | 512 | 40 / 3 | Identique | Identique |
| AArch64 | 4 096 | 5 / 3 | Identique | Identique |

La sonde appelle le moteur dans le service Ring 3 ou EL0 d’une copie temporaire de v0.11.0. Le service bloc, le transport VirtIO et NovaFS proviennent de la base. L’hôte prépare G7. Une VM renomme le répertoire et s’arrête sur un marqueur **après** publication et contrôle. Le banc relève les compteurs via QMP, tue QEMU, vérifie l’image sur macOS et redémarre en RO. Il répète ce cycle pour le renommage inverse. Les trois FLUSH comprennent celui du montage RW et les barrières F1/F2. Dans les quatre couples, la sonde a tourné dans quatre VM successives et chaque remontage RO a laissé l’image identique octet pour octet. La CLI Rosetta a contrôlé les images de l’invité AArch64 et la CLI ARM64 celles de l’invité x86_64.

Les binaires de livraison v0.11.0, sans sonde, ont aussi monté en RO les quatre images G8 et renvoyé leur identifiant de volume via MOUNT et INFO. L’appel public RENAME, les neuf opérations de v0.12.0, les deux répertoires courants simultanés, leurs latences, la mémoire et les fuites de cette future interface restent à qualifier. Les compteurs QMP décrivent des arrêts de VM après les barrières, pas une panne électrique du support physique.

Les suites NovaFS ciblées ont réussi sur ARM64 natif et sous Rosetta, avec neuf tests du contrôleur de clients et trois tests de coupures par architecture. Le contrôleur macOS exerce deux racines privées et des handles mais résout lui aussi les chemins en reconstruisant une chaîne depuis la racine. Le code de NovaFS étant identique à la base, ces essais ciblés constituent la campagne hôte de cet audit. [Banc clients](../../novaFS/tests/clients.rs) · [Banc de coupures](../../novaFS/tests/crash.rs)

### Non-régression sur les binaires de livraison v0.11.0

| Invité | Scénarios historiques | Groupes bloc (deux géométries) | ELF et courses IPC | Contrôles du banc fichier (deux géométries) |
|---|---:|---:|---|---:|
| x86_64 | 25/25 | 24/24 | Les deux suites réussies | 44/44 |
| AArch64 | 25/25 | 24/24 | Les deux suites réussies | 44/44 |

Chaque banc fichier a vérifié 20 coupures, huit erreurs d’E/S avec remontage, l’admission des supports, les refus de pools et la restitution après deux sessions. Le chronométrage et la vérification des ressources de ces suites portent sur v0.11.0. Aucun résultat de ce tableau ne qualifie une opération publique de répertoire.

## Résultat de l’audit de l’interface

- **Processus et volumes.** Le service NovaFS monte un LV autonome. La couche de résolution peut vivre dans ce même processus utilisateur et déléguer les opérations persistantes au moteur. Une instance de montage et un seul écrivain doivent servir les différentes sessions. Les attaches de futurs montages pourront imposer RO et leurs propres droits. Un déplacement entre deux montages doit rendre une erreur explicite. [Intégration et périmètre v0.11.0](validation/novafs-natif-v0.11.0.md) · [Cœur portable](../../novaFS/docs/novafs-coeur-portable.md)
- **Catalogue et identités.** Le format contient déjà la racine (objet 1), les parents, les types, l’index `(parent, nom) → objet`, les révisions et un compteur d’objets croissant avec détection de débordement. `Stat.revision` change lors de certaines mutations. L’identité d’une ouverture doit comprendre l’incarnation du montage et l’objet. Une révision sert au contrôle de concurrence. Un numéro communiqué au client ne confère aucun droit. [Records P0](../../novaFS/src/fs/record.rs) · [Index et tri des noms](../../novaFS/src/tree.rs)
- **Canaux et sessions.** Le lanceur v0.11.0 donne à un seul client une paire privée de capacités IPC. Ses handles de fichier désignent actuellement un chemin dans une table locale de 32 entrées. Le service doit attribuer un état de session, ses droits, son répertoire courant et une table de handles à chaque paire privée. Il doit vérifier l’autorité de traversée sur chaque répertoire, y compris après `..`. Un identifiant dans le payload ne constitue pas une preuve d’identité du client. Un handle fermé ou d’une session révoquée doit rester caduc. [Service actuel](../user/novafs/files.rs) · [Lanceur](../kernel/src/sched/process/ipc/mod.rs) · [Sécurité VFS](../../novaFS/docs/vfs-security.md)
- **Transport et noms.** Les 17 appels système existants suffisent au transport. Un payload IPC de 128 octets laisse 88 octets dans la trame fichier v2. Un chemin, deux opérandes de renommage ou une réponse READDIR peuvent nécessiter plusieurs fragments bornés et propres à la session. Le contrat VFS retient UTF-8 exact, casse significative, absence de normalisation implicite, 255 octets par composant, 32 composants et 1 024 octets par chemin présenté. Rejeter NUL, les composants vides et le séparateur final sauf pour la racine. `.` et `..` sont réservés. Le parcours se fait de gauche à droite avec contrôle d’existence, de type et de droits avant le composant suivant. `..` reste à la racine de confinement quand elle est atteinte. La politique des caractères de contrôle de l’OPEN v2 devra être rendue explicite pour les noms P8 préexistants. [Trame fichier](../abi/files.rs) · [ABI native](../abi/native.rs) · [IPC](ipc-abi.md) · [Contrat des chemins](../../novaFS/docs/vfs-architecture.md)
- **Mutation et récupération.** MKDIR peut réutiliser `Engine::mkdir`. RMDIR et UNLINK doivent contrôler le type puis appeler `Engine::unlink` sous le même écrivain. Un répertoire non vide est refusé. La collecte reprend par curseur pour les fichiers qui détenaient des extents. RENAME réutilise une seule transaction, refuse une destination distincte déjà occupée et un déplacement cyclique. L’index des noms, les parents et les révisions sont publiés ensemble après F1 et F2. L’essai ne met en cause aucune des deux barrières. [Publication](../../novaFS/src/engine.rs) · [Réserve de maintenance](../../novaFS/docs/novafs-p3-espace.md) · [Récupération](../../novaFS/docs/novafs-p1-durabilite.md)
- **Consultation et concurrence.** LOOKUP résout un objet sous la base autorisée. STAT rend type, identité, taille et révision. READDIR utilise le curseur de nom du moteur et doit détecter une révision du répertoire modifiée entre deux pages. CHDIR doit conserver une identité de répertoire dans la session. GETCWD reconstruit le chemin visible depuis la racine de cette session. Pour la première implémentation, refuser par OCCUPÉ la suppression ou le déplacement d’un répertoire courant, d’un objet ouvert ou d’un de leurs ancêtres concernés. Appliquer cette règle à toutes les sessions sous le même écrivain avant publication. [Primitives existantes](../../novaFS/src/fs.rs) · [Règles de concurrence](../../novaFS/docs/vfs-architecture.md)
- **Compatibilité.** Les pools, snapshots, clones, unités comprimées, attestations et clés conservent leurs contrats historiques. Le service v0.11.0 expose uniquement le contenu du LV autonome. Cette expérience porte sur ce profil. [Partage CoW](../../novaFS/docs/novafs-p5-partage.md) · [Compression](../../novaFS/docs/novafs-p6-compression.md) · [Vérification P7](../../novaFS/docs/novafs-p7-verification.md) · [Scrub P8](../../novaFS/docs/novafs-p8-scrub.md)

## Arbitrage proposé

**Option A — borner le chemin canonique de tout objet à 1 024 octets.** Contrôler aussi les descendants au sein de la transaction, y compris lors d’un changement de nom à profondeur constante. Refuser avant F1 si la nouvelle longueur dépasse la borne ou si le budget d’inspection s’épuise. Appliquer la règle aux écrivains macOS et NovaOS. Définir une admission explicite pour les images historiques comme G8 et un accès de récupération RO. L’admission doit distinguer une limite d’accessibilité et une corruption de blocs.

**Option B — borner le chemin fourni à chaque requête et permettre la résolution depuis un répertoire identifié.** Conserver les entrées P8 existantes et ajouter au cœur portable un parcours depuis un objet répertoire validé dans l’instance de montage. Le service conserve l’autorité et les identités en privé. Un client peut atteindre l’objet 6 de G8 en plusieurs requêtes courtes. GETCWD peut rendre par fragments un chemin canonique allant jusqu’à 8 192 octets (32 noms de 255 octets et leurs séparateurs). Il faut distinguer cette borne de la longueur admissible d’une requête d’entrée.

**Recommandation soumise à validation — option B.** Elle permet de monter et de parcourir en lecture seule une image G8 déjà publiée, sans la renommer ni la migrer. Le format sait représenter les cinq objets sous la racine. L’arbitrage concerne le contrat de résolution, les opérations privées du moteur et GETCWD. Dans les deux options, conserver les identités lors du renommage, exercer la révocation sur deux clients, vérifier les refus avant publication, puis reprendre la matrice fonctionnelle et les mesures v0.12.0.

## Reproduction

Depuis la racine du monorepo, les trois blocs sources ci-dessous sont des sondes de qualification pour images jetables. Les commandes extraient la base Git dans un répertoire temporaire. Seule cette copie reçoit la sonde Ring 3 / EL0.

```sh
set -eu
lab=$(mktemp -d)
git archive 24609a52e64a5626527827babc912ba7731d70de novaOS novaFS | tar -x -C "$lab"
python3 - "$lab" <<'PYTHON'
import re, sys
from pathlib import Path
lab = Path(sys.argv[1])
doc = Path("novaOS/docs/namespace-v0.12.0.md").read_text()
for name, lang in (("temoin.rs", "rust"), ("sonde.rs", "rust"), ("qemu-audit.py", "python")):
    block = re.search(r"^#### " + re.escape(name) + r"\n\n`{3}" + lang + r"\n(.*?)^`{3}$",
                      doc, re.M | re.S)
    (lab / name).write_text(block.group(1))
f = lab / "novaOS/user/novafs/files.rs"
s = f.read_text()
line = "            w::set(a, 8, self.id);"
assert s.count(line) == 1
f.write_text(s.replace(line, "            sonde(self.fs.as_mut().unwrap(), self.writable)?;\n"+line)
             + "\n" + (lab / "sonde.rs").read_text())
PYTHON
for arch in aarch64 x86_64; do
    target="$arch-apple-darwin"
    NOVAFS_TARGET="$target" "$lab/novaFS/lab/run" build --lib --bin novafs --offline
    release="$lab/novaFS/target/$target/release"
    rustup run stable rustc --edition=2024 -C opt-level=2 --target "$target" \
        -L "dependency=$release/deps" --extern "novafs=$release/libnovafs.rlib" \
        "$lab/temoin.rs" -o "$lab/temoin-$arch"
    for secteur in 512 4096; do
        for longueur in 205 206 207 255; do
            "$lab/temoin-$arch" "$secteur" "$longueur" "$lab/$arch-$secteur-$longueur.raw"
        done
    done
done
for arch in x86_64 aarch64; do
    make -C "$lab/novaOS" ARCH="$arch" kernel
    case "$arch" in x86_64) hote=aarch64 ;; aarch64) hote=x86_64 ;; esac
    cli="$lab/novaFS/target/$hote-apple-darwin/release/novafs"
    for secteur in 512 4096; do
        python3 "$lab/qemu-audit.py" "$lab" "$arch" "$secteur" "$cli"
    done
done
```

Pour rejouer les suites v0.11.0 sur les sources de livraison, depuis la racine :

```sh
for arch in x86_64 aarch64; do
    make -C novaOS ARCH="$arch" test block-test elf-test ipc-races file-test || exit 1
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline --test clients --test crash || exit 1
done
```

<details>
<summary>Sources des sondes de reproduction</summary>

#### temoin.rs

```rust
use novafs::{Engine, Error, Ids, Kind, Mode, device::FileImage};
use std::{fs::File, io::{Seek, SeekFrom}};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let secteur: u32 = args[1].parse()?;
    let cible: usize = args[2].parse()?;
    let ids = Ids { fs: [1; 16], lv: [2; 16] };
    let mut fs = Engine::format(FileImage::create(256, secteur)?, ids)?;
    let mut parent = String::from("/a");
    fs.mkdir(&parent)?;
    for c in ["b", "c", "d"] {
        parent += &format!("/{}", c.repeat(250));
        fs.mkdir(&parent)?;
    }
    let nom = "e".repeat(64);
    let ancien = format!("{parent}/{nom}");
    fs.create(&ancien)?;
    fs.write(&ancien, 0, b"Nova")?;
    let objet = fs.stat(&ancien)?.id;
    let destination = format!("/{}", "z".repeat(cible));
    fs.rename("/a", &destination)?;
    assert_eq!(fs.generation(), 8);
    let mut support = fs.unmount()?.into_file();
    support.seek(SeekFrom::Start(0))?;
    let mut copie = File::create_new(&args[3])?;
    assert_eq!(std::io::copy(&mut support, &mut copie)?, 256 * 4096);
    copie.sync_all()?;
    drop(copie);
    let ro = Engine::mount(FileImage::from_file(File::open(&args[3])?, 256, secteur)?, ids, Mode::ReadOnly)?;
    ro.verify()?;
    let nouveau_parent = format!("{destination}{}", &parent[2..]);
    let nouveau = format!("{nouveau_parent}/{nom}");
    assert_eq!(ro.stat(&nouveau_parent)?.kind, Kind::Directory);
    assert_eq!(ro.list(&nouveau_parent, None, 2)?, [nom]);
    assert!(matches!(ro.stat(&ancien), Err(Error::NotFound)));
    if nouveau.len() <= 1024 {
        assert_eq!(ro.stat(&nouveau)?.id, objet);
        assert_eq!(ro.read(&nouveau, 0, 4)?, b"Nova");
    } else {
        assert!(matches!(ro.stat(&nouveau), Err(Error::Invalid("chemin"))));
        if nouveau.len() == 1025 {
            assert_eq!(ro.read(&nouveau[1..], 0, 4)?, b"Nova");
        } else {
            assert!(matches!(ro.read(&nouveau[1..], 0, 4), Err(Error::Invalid("chemin"))));
        }
    }
    drop(ro);
    let mut rw = Engine::mount(FileImage::from_file(support, 256, secteur)?, ids, Mode::ReadWrite)?;
    rw.rename(&destination, "/a")?;
    let final_ro = Engine::mount(rw.unmount()?, ids, Mode::ReadOnly)?;
    final_ro.verify()?;
    assert_eq!(final_ro.stat(&ancien)?.id, objet);
    assert_eq!(final_ro.read(&ancien, 0, 4)?, b"Nova");
    println!("secteur={secteur} chemin={} objet={objet} générations=7,8,{} retour=Nova", nouveau.len(), final_ro.generation());
    Ok(())
}
```

#### sonde.rs

```rust
fn sonde(fs: &mut Engine<&IpcDevice<'_>>, ecriture: bool) -> rt::Result<()> {
    let ancien_parent = alloc::format!("/a/{}/{}/{}", "b".repeat(250), "c".repeat(250), "d".repeat(250));
    let dest = alloc::format!("/{}", "z".repeat(255));
    let parent = alloc::format!("{dest}{}", &ancien_parent[2..]);
    let nom = "e".repeat(64);
    let ancien = alloc::format!("{ancien_parent}/{nom}");
    let nouveau = alloc::format!("{parent}/{nom}");
    let court = |fs: &Engine<&IpcDevice<'_>>| -> rt::Result<()> {
        shape(fs.stat(&ancien).map_err(error)?.id == 6 && fs.read(&ancien, 0, 4).map_err(error)? == b"Nova")
    };
    let long = |fs: &Engine<&IpcDevice<'_>>| -> rt::Result<()> {
        shape(fs.list(&parent, None, 2).map_err(error)? == [nom.as_str()]
            && matches!(fs.stat(&ancien), Err(Error::NotFound))
            && matches!(fs.stat(&nouveau), Err(Error::Invalid("chemin")))
            && matches!(fs.read(&nouveau, 0, 4), Err(Error::Invalid("chemin"))))
    };
    let marque: &[u8] = match (fs.generation(), ecriture) {
        (7, true) => {
            court(fs)?;
            fs.rename("/a", &dest).map_err(error)?;
            long(fs)?;
            b"AUDIT G8 rename confirme"
        }
        (8, false) => { long(fs)?; b"AUDIT G8 entree listee lecture refusee" }
        (8, true) => {
            long(fs)?;
            fs.rename(&dest, "/a").map_err(error)?;
            court(fs)?;
            b"AUDIT G9 retour confirme"
        }
        (9, false) => { court(fs)?; b"AUDIT G9 objet 6 Nova" }
        _ => return Err(w::CORRUPT),
    };
    fs.verify().map_err(error)?;
    if ecriture { crate::qualification::stop(marque); }
    rt::value(rt::call(rt::DEBUG_WRITE, marque.as_ptr() as u64, marque.len() as u64, 0))?;
    Ok(())
}
```

#### qemu-audit.py

```python
from pathlib import Path
import re
import subprocess
import sys
import tempfile
base = Path(sys.argv[1]).resolve()
sys.path.insert(0, str(base / "novaOS/tests"))
from bloc import Bench
arch = sys.argv[2]
secteur = int(sys.argv[3])
cli = str(Path(sys.argv[4]).resolve())
def outil(cmd, *args, data):
    return subprocess.run([cli, cmd, "256", str(secteur), *args], input=data,
                          capture_output=True, check=True).stdout
parent = "/a"
cmds = ["mkdir /a"]
for ch in "bcd":
    parent += "/" + ch * 250
    cmds.append("mkdir " + parent)
ancien = parent + "/" + "e" * 64
cmds += ["create " + ancien, "write " + ancien + " 0 4e6f7661"]
def oracle(image, attendu):
    contenu = image.read_bytes()
    ancres = re.findall(r"slot=(\d+) génération=(\d+) blocs=\d+ FS=(\w+) LV=(\w+)",
                        outil("inspect", data=contenu).decode())
    assert len(ancres) == 2
    slot, gen, fs, lv = max(ancres, key=lambda x: int(x[1]))
    assert int(gen) == attendu
    outil("verify", fs, lv, data=contenu)
    if attendu in (7, 9):
        assert outil("salvage", fs, lv, slot, ancien, data=contenu) == b"Nova"
    return contenu
with tempfile.TemporaryDirectory(prefix="nova-noms-") as tmp:
    b = Bench(Path(tmp), arch, secteur)
    b.image.write_bytes(outil("script", data=("\n".join(cmds)+"\n").encode()))
    oracle(b.image, 7)
    for gen, mutation, lecture in [(8, "AUDIT G8 rename confirme", "AUDIT G8 entree listee lecture refusee"),
                                  (9, "AUDIT G9 retour confirme", "AUDIT G9 objet 6 Nova")]:
        b.run("files-inject", f"g{gen}-mutation", cut=mutation)
        avant = oracle(b.image, gen)
        s = b.run("files-info", f"g{gen}-reprise", readonly=True)
        assert lecture in s and "BLOC session terminee ok=true" in s, s[-1800:]
        assert b.image.read_bytes() == avant
        print(arch, secteur, gen, lecture, flush=True)
```

</details>
