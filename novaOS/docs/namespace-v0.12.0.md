# Espace de noms natif — audit de v0.12.0

## Reprise depuis `6a5656c` — compatibilité de la profondeur

**La résolution depuis une capacité de répertoire est validée comme choix d’architecture.** Le point restant à arbitrer est la profondeur d’une arborescence persistante dans un seul LV. Un lecteur NovaFS v0.9.12 refuse le montage au-delà de **32 arêtes** depuis la racine, même si les objets, les noms et les deux générations ont des sommes correctes. La longueur canonique maximale du profil est donc **32 × (255 + 1) = 8 192 octets**, séparateurs compris. Un chemin canonique de 10 001 octets exige au moins 40 arêtes. Le découpage en requêtes de 1 024 octets préserve le budget IPC et laisse intact le contrôle du montage. [Contrat du format, catalogue et admission](../../novaFS/docs/novafs-disk-format.md) · [Vérification des objets vivants](../../novaFS/src/fs/check.rs)

### Origine des limites

| Couche | Contrôle observé | Conséquence |
|---|---|---|
| API publique et résolution | `components` refuse une entrée de plus de 1 024 octets et plus de 32 composants. `resolve` commence à l’objet racine. Le curseur de lecture mémorise facultativement un chemin déjà accepté. | La limite de 1 024 porte sur l’argument. Les appels successifs actuels recommencent à la racine. [Résolution](../../novaFS/src/fs.rs) · [Lecture et cache du chemin](../../novaFS/src/fs/data.rs) |
| Index et sérialisation | La clé de nom contient le parent u64, la longueur u16 et 1 à 255 octets UTF-8. Sa valeur est l’ID enfant u64. L’enregistrement d’objet porte le parent, le nom, le type et la révision. Le compteur `next_oid` interdit le recyclage d’un numéro au sein de la lignée validée. | Le moteur peut rechercher un enfant depuis un parent. La limite de niveaux internes du B+Tree concerne l’index. Le chemin canonique n’occupe aucun champ sur disque. [Records](../../novaFS/src/fs/record.rs) · [Comparateur et codec B+Tree](../../novaFS/src/tree.rs) |
| Écriture et montage | La création contrôle la profondeur du parent. Le renommage vérifie aussi les descendants lorsqu’il augmente la profondeur. Le montage contrôle les parents de chaque objet vivant dans chacune des deux générations admises. | Un écrivain qui produirait un objet de profondeur 33 créerait un volume refusé par les lecteurs actuels. Les mêmes champs binaires peuvent encoder cette chaîne, mais sa publication changerait le contrat de compatibilité. [Mutations](../../novaFS/src/fs.rs) · [Contrôle des générations](../../novaFS/src/engine.rs) |
| Adaptateur natif et IPC | Le cœur `no_std` partage les fonctions précédentes avec Darwin. Le `Device` transporte des blocs de 4 Kio. Les messages fichier v2 ont 128 octets dont 88 de charge utile, et OPEN limite actuellement son argument à une trame. | La fragmentation d’une requête pourra franchir 88 octets. La géométrie de secteur et les tampons de transport n’interviennent pas dans le calcul de profondeur. [Adaptateur](../user/novafs/device.rs) · [Trame fichier](../abi/files.rs) · [Client](../user/storage/fileapi.rs) |
| Tests historiques | Le scénario hôte V02 crée 32 répertoires, exige le rejet du suivant et exerce une entrée de 1 024 octets. | Le maintien des tests et de l’interopérabilité couvre un rejet observable. [V02](../../novaFS/tests/clients.rs) |

### Résultats de la nouvelle campagne

1. **Objet renommé de 820 à 1 074 octets.** Sur ARM64 natif et x86_64 sous Rosetta, en secteurs de 512 puis de 4 096 octets, la sonde mémorise l’ID du fichier avant le renommage. Après publication et remontage RO, elle suit cinq arêtes dans l’index des noms, vérifie chaque lien inverse et retrouve le même ID. La lecture de l’extent avec son checksum rend les quatre octets `4e 6f 76 61`. Aucun renommage inverse ni reconstruction du chemin complet n’intervient dans cette lecture. La sonde exerce les primitives internes du moteur avec le profil de production. La délégation IPC d’une capacité de répertoire reste à implémenter.
2. **Longueurs canoniques.** Pour chaque couple hôte/secteur, la CLI de production crée puis vérifie après remontage des images aux longueurs 1 023, 1 024, 1 025, 1 074, 4 096 et 8 192 octets. La lecture par chemin absolu retourne `Nova` dans les deux premiers cas puis `Invalid("chemin")` dans les quatre autres. Pour le témoin de 10 001 octets, le générateur expérimental assemble 40 composants, publie et vérifie le catalogue. Les deux lecteurs de production refusent ensuite ce volume au contrôle de profondeur.
3. **Écrivain, lecteur et reprise.** Le test déplace `/s`, contenant le fichier `f`, sous une chaîne de répertoires nommés `x`. Sous 30 répertoires, le fichier atteint la profondeur 32 et le renommage réussit. Sous 31 répertoires, l’opérande de destination fait 64 octets et compte 32 composants. L’écrivain de production refuse cette mutation avec `Limit("profondeur après renommage")`. Un générateur temporaire autorisant 40 niveaux publie la même mutation. Il relit `Nova` à la génération 36. Le vérificateur v0.9.12 refuse le montage normal avec `Corrupt("cycle ou profondeur du catalogue")`. Le sauvetage RO du prédécesseur explicitement choisi retrouve `/s/f` en G35. Une rétrogradation automatique vers cette ancienne génération ferait disparaître un renommage confirmé par le générateur.

| Invité de production | Secteur | 32 arêtes, G35 | 33 arêtes, G36 | 40 arêtes, chemin de 10 001 octets |
|---|---:|---|---|---|
| x86_64 | 512 | MOUNT RO et INFO réussis | RO et RW refusés, statut 21 | RO refusé, statut 21 |
| x86_64 | 4 096 | MOUNT RO et INFO réussis | RO et RW refusés, statut 21 | RO refusé, statut 21 |
| AArch64 | 512 | MOUNT RO et INFO réussis | RO et RW refusés, statut 21 | RO refusé, statut 21 |
| AArch64 | 4 096 | MOUNT RO et INFO réussis | RO et RW refusés, statut 21 | RO refusé, statut 21 |

Le bloc de commandes de reproduction ci-dessous a été exécuté depuis une extraction neuve de la base. Le banc compare chaque image octet pour octet après chacune des 16 VM, y compris lors d’une tentative RW refusée. Les images à 33 et 40 arêtes sont des contre-exemples fabriqués dans une copie temporaire du code. Le seul paramètre changé dans le générateur est `MAX_DEPTH`. Toutes les admissions QEMU ci-dessus utilisent le noyau et le service de la base. [Reproducteur](../tests/profondeur_p8.py)

### Régressions rejouées depuis la base de reprise

| Architecture | Scénarios historiques QEMU | Groupes bloc | Contrôles fichier v0.11.0 | ELF / courses IPC | macOS clients / coupures |
|---|---|---:|---:|---|---|
| x86_64 | 25/25 au premier passage | 24/24 | 44/44 | réussis | 9/9 et 3/3 sous Rosetta |
| AArch64 | Premier passage interrompu au scénario processus, puis 25/25 au rejeu complet | 24/24 | 44/44 | réussis | 9/9 et 3/3 en natif |

Lors du premier passage AArch64, le PID 14 avait annoncé sa sortie et le noyau a paniqué sur l’assertion `orphelin non collecté` de `proc-test`. Le test exige la collecte dans un délai de `HZ` ticks. Le même binaire et le même test ont réussi au rejeu. La cause de cet échec intermittent reste ouverte. Les contrôles bloc et fichier couvrent les deux géométries, les coupures et les erreurs de FLUSH. Les suites P8 complètes, instrumentées et millionnaires n’ont pas été relancées. Le moteur de production et ses codecs n’ont reçu aucune modification. [Assertion du noyau](../kernel/src/sched/process/demo.rs) · [Banc QEMU](../tests/demarrage.py)

### Décision à prendre avant implémentation

- **Conserver P8 en v0.12.0 (recommandé).** Autoriser les requêtes de 1 024 octets depuis une capacité pour atteindre tout objet admis, y compris les fichiers dont le chemin canonique fait 1 074, 4 096 ou 8 192 octets. Garder la borne persistante de 32 arêtes et qualifier le refus au-delà. Dans ce périmètre, un parcours répété de 100 ou 1 000 composants peut mesurer des appels successifs, mais ne démontre pas une arborescence de 100 ou 1 000 niveaux distincts.
- **Exiger plus de 32 niveaux distincts sur un même volume.** Autoriser une évolution explicite et détectable du profil de stockage. Elle devra conserver la lecture des images P8 et permettre aux anciens lecteurs de refuser le nouveau profil avant mutation. Remplacer la borne fixe par une détection de cycles et des budgets d’inspection vérifiables à l’admission, au renommage et à la récupération. Les modifications de format et toute migration demandent une décision séparée.

Dans les deux cas, la base d’une requête sera un handle lié à un montage, à une session authentifiée par son canal et à ses droits effectifs. L’ID sur disque sert à la recherche interne. LOOKUP_AT et OPEN_AT devront vérifier la traversée à chaque étape. Une capacité ouverte restera attachée à son objet si le renommage reste dans son périmètre. Un déplacement qui ferait sortir un objet ouvert ou le répertoire courant de ce périmètre devra être refusé avant publication. Toute remontée par `..` devra respecter la racine de confinement de la capacité, ou être refusée. CHDIR conservera l’identité en session et GETCWD pourra produire le texte par fragments. Le service utilisera les transactions existantes pour MKDIR, RMDIR, UNLINK et RENAME. Ces contrats portent sur l’implémentation à reprendre après arbitrage.

Les latences, les allocations et les pics mémoire des futures opérations `*_AT` restent à mesurer. Aucune mesure de montage négatif n’est présentée comme la performance d’une opération de répertoire native.

### Reproduire le contrôle de compatibilité

Depuis la racine de la livraison documentaire, sur macOS ARM64 avec Rosetta et les outils de développement du projet :

```sh
set -eu
base=6a5656cbc7dab65320aa417f4298b7e39c412d67
lab=$(mktemp -d)
git archive "$base" novaFS | tar -x -C "$lab"
python3 - "$lab" <<'PY'
from pathlib import Path
import sys
doc = Path("novaOS/docs/namespace-v0.12.0.md").read_text()
probe = doc.split("#### identite_p8.rs\n\n```rust\n", 1)[1].split("\n```", 1)[0]
with (Path(sys.argv[1]) / "novaFS/src/fs/tests.rs").open("a") as tests:
    tests.write("\n" + probe + "\n")
PY
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" "$lab/novaFS/lab/run" test --offline --lib \
        fs::tests::acces_par_identite_apres_renommage -- --exact
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run build --offline --bin novafs
    make -C novaOS ARCH="$arch" kernel
done
python3 - "$lab" <<'PY'
from pathlib import Path
import sys
f = Path(sys.argv[1]) / "novaFS/src/fs/record.rs"
old = "const MAX_DEPTH: usize = 32;"
s = f.read_text()
assert s.count(old) == 1
f.write_text(s.replace(old, "const MAX_DEPTH: usize = 40;"))
PY
NOVAFS_TARGET=aarch64-apple-darwin "$lab/novaFS/lab/run" build --offline --bin novafs
for arch in aarch64 x86_64; do
    python3 novaOS/tests/profondeur_p8.py "$arch" "$lab/novaFS/target/aarch64-apple-darwin/release/novafs"
done
```

<details>
<summary>Sonde de lecture par identité dans la copie temporaire du moteur</summary>

#### identite_p8.rs

```rust

#[test]
fn acces_par_identite_apres_renommage() -> Result<()> {
    use crate::device::FileImage;
    for secteur in [512, 4096] {
        let mut fs = Engine::format(FileImage::create(256, secteur)?, IDS)?;
        let noms = ["a".to_owned(), "b".repeat(250), "c".repeat(250), "d".repeat(250), "e".repeat(64)];
        let mut parent = String::new();
        for nom in &noms[..4] {
            parent.push('/');
            parent.push_str(nom);
            fs.mkdir(&parent)?;
        }
        let ancien = format!("{parent}/{}", noms[4]);
        assert_eq!(ancien.len(), 820);
        fs.create(&ancien)?;
        fs.write(&ancien, 0, b"Nova")?;
        let id = fs.stat(&ancien)?.id;
        let nouveau_nom = "z".repeat(255);
        fs.rename("/a", &format!("/{nouveau_nom}"))?;
        let fs = Engine::mount(fs.unmount()?, IDS, Mode::ReadOnly)?;
        fs.verify()?;
        assert_eq!(ancien.len() - 1 + nouveau_nom.len(), 1074);
        let mut objet = live(&fs, ROOT)?;
        for nom in core::iter::once(&nouveau_nom).chain(&noms[1..]) {
            assert_eq!(objet.stat.kind, Kind::Directory);
            let parent = objet.stat.id;
            let valeur = get(&fs, Family::Names, &name_key(parent, nom))?.expect("entrée présente");
            objet = live(&fs, number(&valeur)?)?;
            assert_eq!((objet.parent, &objet.name), (parent, nom));
        }
        assert_eq!((objet.stat.id, objet.stat.kind, objet.stat.size), (id, Kind::File, 4));
        let (debut, extent) = data::covering(&fs, id, 0)?.expect("extent présent");
        assert_eq!(debut, 0);
        let page = data::checked_page(&fs, extent.reference(0), 0, objet.stat.size)?;
        assert_eq!(&page[..4], b"Nova");
    }
    Ok(())
}
```

</details>

## Audit initial sur la base `24609a5`

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

## Arbitrage initial (option B autorisée lors de la reprise)

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
