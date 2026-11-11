# NovaFS P7 — audit préalable du chiffrement authentifié

**Décision soumise à arbitrage.** Deux copies d'une image P6 peuvent être ouvertes séparément en écriture. Si le futur pool chiffré conserve cette autonomie, la copie dupliquera aussi le domaine cryptographique et l'état persistant de son compteur. Sa prochaine valeur pourra alors être employée deux fois. Le CSPRNG indépendant de macOS permettrait de borner très fortement le risque de collision, mais deux tirages de 192 bits peuvent être égaux. Le §7 demande à la fois cette méthode et l'impossibilité de réutiliser une paire clé/nonce pour deux contenus distincts. Les §3, §17 et §19 imposent de trancher cette différence avant de publier un écrivain. Le livrable est l'audit du commit `ea00a755deeb6e5fde64be3ebb748912c6089dc7` sur `main`. Le manifeste et le verrou identifient v0.9.8. La licence de NovaFS est la MPL-2.0. Le SHA-256 vérifié de son texte est `fab3dd6bdab226f1c08630b1dd917e11fcb4ec5e1e020e2c16f83a0a13863e85`. [Manifeste](../Cargo.toml) · [Licence](../LICENSE)

## 1. Témoin sur le format actuel

Un pool P6 de 1 024 blocs contient une origine de 256 blocs et un fichier vide. Le laboratoire a soumis deux écritures légitimes partant exactement de la même image de génération 3. L'une contient 4 096 octets `41` hexadécimal, l'autre autant d'octets `42`. Chaque sortie a passé `check-pool --full`, puis une relecture exacte du fichier. Le parcours du catalogue sélectionné et des feuilles de traduction a identifié l'allocation suivante dans les deux sorties.

| Exécutable macOS | Secteur | Génération des deux sorties | LBA du contenu | Adresse physique | Contrôles complets / relectures |
| --- | ---: | ---: | ---: | ---: | ---: |
| ARM64 natif | 512 | 4 | 9 | 22 | 2 / 2 |
| ARM64 natif | 4 096 | 4 | 9 | 22 | 2 / 2 |
| x86_64 sous Rosetta | 512 | 4 | 9 | 22 | 2 / 2 |
| x86_64 sous Rosetta | 4 096 | 4 | 9 | 22 | 2 / 2 |

Les identités du pool, du FS et du LV concordaient aussi. La source a conservé son SHA-256. Ce résultat porte sur l'allocateur et les générations P6. Aucune opération AEAD n'a eu lieu. Une dérivation déterministe de clé et de nonce à partir de ces seuls champs produirait la même paire sur les deux branches. La copie entière peut être faite hors du service, même si le service sérialise parfaitement les écritures sur chaque branche. [Sélection et allocation du graphe](../src/shared_graph.rs) · [Contrat CoW et reçus](novafs-p5-partage.md)

Il existe également une fenêtre sur **une seule image**. Une préparation peut écrire un chiffré que le support conserve alors que son catalogue ne sera jamais publié. Si le compteur correspondant ne devient durable qu'avec ce catalogue, la reprise pourra réattribuer sa valeur. Le premier chiffré a pu être observé. Compter uniquement les mutations acquittées ou rechercher des doublons dans les deux générations vivantes laisse échapper ces appels. Exclure la détection du rollback intégral du modèle de fraîcheur ne suffit pas à supprimer ce risque pour la confidentialité de deux écritures. [Protocole F1/F2](novafs-p1-durabilite.md) · [Réservation des nonces, RFC 5116 §3.1](https://www.rfc-editor.org/rfc/rfc5116.html#section-3.1)

## 2. Trois contrats possibles

| Option | Garantie et hypothèses | Conséquence pour NovaFS |
| --- | --- | --- |
| **A. XChaCha20-Poly1305, nonce aléatoire à chaque chiffrement** | Sous une même clé, pour q tirages indépendants et uniformes de 24 octets, la probabilité d'au moins une collision est au plus q(q−1)/2¹⁹³. Pour deux appels elle vaut 2⁻¹⁹². Jusqu'à 2³² appels, elle reste inférieure à 2⁻¹²⁹. | Recommandation pour garder les images autonomes. Accepter explicitement une garantie probabiliste. L'hôte doit fournir de l'entropie fraîche après reprise. Un échec du CSPRNG interrompt l'opération avant toute sortie chiffrée. |
| **B. XChaCha20-Poly1305, attribution stricte par clé** | Tous les écrivains du domaine obtiennent des valeurs disjointes d'un état monotone durable qui échappe à la copie et au rollback de l'image. Toute valeur attribuée est consommée avant chiffrement, même en cas d'abandon. | Conserve l'exigence « jamais ». Il faut définir le détenteur de l'état, le transfert entre machines et le refus d'écriture lorsqu'il est indisponible. Une plage réservée devient dangereuse si deux écrivains peuvent reprendre la même mémoire vive. Exclure le clonage de l'état vif entre attribution et emploi, ou effectuer l'attribution et le chiffrement dans le composant non clonable. |
| **C. AEAD résistant à la répétition accidentelle** | AES-256-GCM-SIV est une construction normalisée qui limite les conséquences d'une répétition de nonce. Elle conserve des limites d'usage et peut révéler l'égalité des messages en cas de répétition dans le même contexte. | Changer la suite et la propriété à valider. Cette option ne garantit pas l'unicité. Elle exige un nouvel audit du format, des limites de la primitive et des bibliothèques. |

Dans A, q additionne les tentatives réussies, avortées et jamais publiées, sur toutes les copies qui utilisent **la même clé effective**. Le seuil de 2³² illustre la borne. Un compteur de rotation local ne peut imposer ce plafond à une collection inconnue de copies. Les risques calculés par clé s'additionnent pour borner le risque global. La probabilité de collision du nonce ne mesure ni la résistance du mot de passe ni la probabilité de falsification d'un tag. Les snapshots qui reprennent les mêmes octets chiffrés n'invoquent pas le chiffrement. Toute opération qui **rechiffre**, y compris un nouvel essai, un emplacement de clé ou un enregistrement d'export, est soumise à la politique de sa clé. [Construction XChaCha, document de conception](https://datatracker.ietf.org/doc/html/draft-irtf-cfrg-xchacha-03#section-2.1) · [Usage de nonces aléatoires par libsodium](https://doc.libsodium.org/secret-key_cryptography/aead/chacha20-poly1305/xchacha20-poly1305_construction) · [Limites d'AES-GCM-SIV, RFC 8452 §9](https://www.rfc-editor.org/rfc/rfc8452.html#section-9)

## 3. Frontières de confiance

L'adversaire peut conserver plusieurs versions de l'image, lire chaque secteur, changer les en-têtes et les références, réordonner ou supprimer des blocs et présenter une archive modifiée. Il peut tenter des phrases secrètes hors ligne. Le service, son code de vérification, les clés effectivement autorisées, le CSPRNG de l'hôte et la mémoire d'un processus déverrouillé sont dans le périmètre de confiance. L'accès physique, les allocations, les tailles, les trous, la topologie publique et le gain de compression pourront être observables. Un service d'écriture accessible à un acteur non fiable ne doit pas mélanger ses données et des secrets d'un autre domaine dans une unité comprimée.

Pour une racine authentifiée, le lecteur doit détecter toute substitution ou modification des octets utilisés pour produire son résultat, sous les hypothèses des primitives. Un adversaire peut toujours empêcher la lecture en détruisant un secteur. Un reçu ancien authentique peut encore authentifier une ancienne image entière. Exiger le dernier état confirmé suppose un témoin récent, indépendant du pool et résistant au recul, conservant identité, génération et empreinte de racine. Une sauvegarde locale du même reçu peut reculer avec l'image. La révocation à travers toutes les copies a la même limite. [Limites de la récupération NovaFS](novafs-recovery.md)

## 4. Clés, déverrouillage et confinement

- À la création, chaque **origine chiffrée indépendante** reçoit un identifiant de domaine et une clé maîtresse de 32 octets tirée du CSPRNG. Un snapshot et ses clones partagent ce domaine, y compris pour les écritures divergentes de ses clones. Un détenteur de la clé du domaine peut lire hors ligne ce que les clés de ce domaine protègent. L'autorisation d'un client sur une vue reste du ressort du service.
- HKDF-SHA-256 dérive des clés distinctes pour les données, les métadonnées confidentielles, le MAC des racines et l'enveloppement des clés d'export. Les contextes de dérivation incluent un libellé versionné, le domaine et l'époque. L'encodage des champs utilise des tailles fixes ou un préfixe de longueur. Un secret administratif de pool distinct authentifie le catalogue global et l'allocation des domaines. La clé administrative doit être provisionnée localement à la création, enveloppée pour l'administrateur et tenue distincte des secrets de déverrouillage des domaines. Un lecteur de volume peut vérifier sa racine avec ses propres clés. Modifier la répartition physique globale requiert l'autorité administrative. [HKDF, RFC 5869](https://www.rfc-editor.org/rfc/rfc5869.txt)
- Deux emplacements indépendants enveloppent la même clé maîtresse avec AEAD. Le premier utilise une KEK dérivée par Argon2id de la phrase secrète. Le second utilise une KEK dérivée par HKDF d'un secret de récupération aléatoire de 32 octets remis séparément au propriétaire. Les emplacements lient dans leurs données associées le type, le domaine, l'époque, le numéro d'emplacement, la version et les paramètres de dérivation. L'identité de domaine attendue est fournie par le contexte de confiance. Le manifeste authentifié du domaine engage l'ensemble des emplacements autorisés. Après ouverture d'une enveloppe, vérifier cet ensemble avant d'accepter la clé. Une clé incorrecte donne un refus uniforme. Des identifiants futurs pourront désigner NovaKey ou un authentificateur matériel.
- Profil de phrase proposé pour l'outil hôte de déverrouillage : Argon2id v19, sel frais de 16 octets, sortie de 32 octets, 64 Mio, trois passes et quatre voies. Accepter uniquement les profils connus, borner les longueurs *avant* allocation et sérialiser les dérivations coûteuses. La matrice de 64 Mio dépasse à elle seule le budget monté de 7 Mio. Elle doit être explicitement comptabilisée dans l'outil de déverrouillage, avec son propre pic et sa latence. Si le plafond devait aussi englober cet outil, il faudrait arbitrer le profil avant toute livraison par phrase secrète. [Argon2id, RFC 9106 §4](https://www.rfc-editor.org/rfc/rfc9106.html#section-4)
- Une rotation du mot de passe remplace l'enveloppe au moyen d'une transaction. Une rotation de données utilise une nouvelle époque avec des clés appropriées. L'ancienne époque doit rester lisible tant que des références protégées l'utilisent. Si la clé maîtresse a fuité, changer le libellé d'époque sous cette même clé ne rétablit aucune confidentialité. Il faut de nouveaux secrets indépendants et un réencodage explicite pour protéger des données futures contre son ancien détenteur. Une ancienne sauvegarde et ses anciens secrets peuvent toujours être utilisés ensemble. La perte de la phrase reste récupérable avec le second emplacement. La perte de tous les moyens de déverrouillage et de leurs sauvegardes est définitive.
- Entrée du secret via un canal dédié et borné. Exclure arguments CLI, environnement, traces, JSON et temporaires en clair. Garder les secrets dans des tampons à propriétaire unique et limiter les copies. Le verrouillage révoque les tickets et accès en vol, attend les opérations admises, purge aussi le cache logique, les pages privées et le tampon Argon2 avant libération. Répéter l'effacement sur erreur. Les copies de l'appelant, registres, crash dumps et swap macOS limitent la portée du nettoyage. [Cache P4](novafs-p4-cache.md) · [Garanties de zeroize](https://docs.rs/zeroize/1.9.1/zeroize/)

## 5. Format proposé, opérations CoW et vérification

Un pool P7 porterait une signature distincte, une révision explicite et des fonctionnalités obligatoires. L'entête borne la géométrie, les algorithmes, le nombre d'époques et d'emplacements. Une référence publique authentifiée associe le domaine à sa politique, à ses emplacements de clé et à sa racine. Le lecteur qui attend un volume protégé refuse une image se présentant comme non chiffrée ou de version antérieure. Les lecteurs historiques P0–P6 restent distincts. Une migration éventuelle doit produire une nouvelle image, vérifiée avant remise au client. L'original et ses anciennes copies peuvent contenir du clair.

Une page brute P6 occupe déjà 4 096 octets, tout comme une unité LZ4 contenant de 2 à 16 pages logiques. Une unité P7 pourrait donc garder son chiffré dans un bloc de 4 Kio et placer sa version, son identifiant immuable, sa portée, son nonce de 24 octets et son tag de 16 octets dans des métadonnées CoW séparées. Ajouter simplement 40 octets aux 64 références d'une feuille P6 nécessiterait 5 152 octets avec son en-tête de 32 octets. Une page auxiliaire liée à la feuille, ou une nouvelle géométrie de feuilles, doit entrer dans le calcul du crédit avant toute écriture. La réservation couvre les pages incompressibles, les métadonnées cryptographiques, une divergence CoW et les deux générations. Le registre doit épingler le chiffré et sa description jusqu'au retrait de leur dernière référence. [Format des feuilles P5](novafs-p5-format.md) · [Unités P6](novafs-p6-compression.md) · [Admission physique P3](novafs-p3-espace.md)

L'identité d'une unité est attribuée à sa création. Les données associées encodent en ordre canonique la version, la suite, le pool, le domaine, l'époque, le type, l'identité immuable, l'adresse physique autorisée, la portée logique originale, la longueur logique et la longueur encodée. Pour un pack partagé, la portée désigne les slots de **l'unité originale**, dans l'ordre, et chaque arête doit nommer un slot présent. Le catalogue authentifie séparément le lien entre la vue, son propriétaire, son type, son domaine et sa racine. Un clone réutilise le chiffré, le nonce et le tag existants. Une réécriture partielle construit une nouvelle unité. La suppression de l'origine laisse intactes la clé enveloppée du domaine et les données encore retenues par un descendant ou le catalogue précédent.

L'écrivain compresse d'abord les données lorsque le gain physique le justifie. Il authentifie et chiffre ensuite le bloc complet, y compris le répertoire du pack et son remplissage. Les noms, extents, métadonnées du FS interne et empreintes logiques susceptibles de révéler un contenu connu doivent eux aussi rester confidentiels sur le support. Les champs nécessaires à la navigation publique peuvent rester visibles si leur liaison à une racine est authentifiée. À la lecture, borner les champs publics, reconstruire les données associées depuis la référence attendue et vérifier le tag via l'API AEAD. Toute sortie temporaire reste privée et est effacée sur erreur. Décompresser, vérifier les tailles et les contraintes FS, puis rendre les octets ou les admettre au cache.

Le contrôle **sans clé** peut vérifier signatures, tailles, plages, compteurs, types et canonicalité. Son résultat doit indiquer « structure examinée, authenticité non vérifiée ». Le contrôle **avec clés** vérifie la racine administrative, les racines de domaines, les deux générations, les tags, les identités, les arêtes et enfin le contenu logique décompressé. Il rapporte les domaines réellement couverts. Une mutation exige une authentification suffisante de l'allocation globale, même si un autre domaine est verrouillé. En l'absence des clés ou d'un index authentifié suffisant pour épingler ses blocs, refuser l'écriture. Un défaut critique bloque la publication et invalide les données en cache concernées. Aucune substitution silencieuse d'un trou, d'une ancienne page ou de zéros.

## 6. Engagement et export

F1 doit stabiliser ensemble les nouveaux chiffrés, tags, métadonnées CoW, références, emplacements modifiés et catalogue candidat. L'autorisation authentifiée de publier cette racine intervient après F1. F2 la stabilise avant tout acquittement. Un candidat correctement authentifié peut être présent sur disque avant publication. Sa seule présence ne permet pas au montage de le sélectionner. Les quatre copies du code Gray P5 assurent une propriété de déchirure sous leur modèle de panne. Elles ne prouvent pas qu'un détenteur de la clé de publication a choisi ce catalogue. Le format P7 devra démontrer la liaison entre sélecteur et racine sous écriture partielle et rejouer les frontières F1/F2 en 512 et 4 096 octets. Si la preuve est indéterminée, le contrôle refuse l'écriture et expose un diagnostic. [Publication existante](../src/shared_graph.rs) · [Reprise P1](novafs-p1-durabilite.md)

L'archive autonome aurait une suite et une version propres. Une clé de transport fraîche protège les enregistrements. Elle est enveloppée pour un secret de sauvegarde autorisé, disponible indépendamment du pool source. Le manifeste authentifié lie l'identité d'archive, le domaine source, la vue, le reçu, la géométrie, les deux générations et le nombre d'enregistrements. Chaque tag couvre également la position, le LBA et la longueur attendue. Une clôture authentifiée couvre le manifeste, le nombre et l'ordre des enregistrements avec leur empreinte, et la fin de flux doit être exacte. À la restauration, vérifier les bornes, les tags et la clôture avant d'accepter l'archive ou d'engager la destination. Le moteur pourra relire une copie temporaire du **flux chiffré** sur descripteur exclusif et utiliser une page claire bornée en RAM. Les enregistrements logiques pourront alors être réencodés dans un domaine et un pool neufs. Le contrat de sauvegarde P5 conserve les identités FS/LV de lignée et attribue à la vue restaurée une nouvelle identité publique (pool, numéro de vue). Son domaine cryptographique de destination peut être neuf. Le détenteur de la clé d’archive peut fabriquer une archive valide. Pour prouver sa provenance à un tiers, conserver le reçu attendu par un canal de confiance indépendant. Un secret d'archive perdu rend cette sauvegarde inutilisable.

L'export v1 transmet des pages logiques en clair et son lecteur les matérialise dans un fichier temporaire. Ni ce chemin, ni un export silencieux v1, ne conviennent à une vue chiffrée. L'archive autonome inclut potentiellement des octets d'un fichier supprimé, encore retenus par le prédécesseur. L'authenticité de l'archive n'atteste pas qu'il s'agit de la plus récente. [Export P5 et contrat A](novafs-p5-export.md) · [Chemins de sérialisation et de restauration](../src/shared_export.rs)

## 7. Dépendances examinées

| Usage | Candidat Rust | Licence et trajectoire |
| --- | --- | --- |
| AEAD, tag détaché | `chacha20poly1305` 0.11.0, XChaCha20-Poly1305 à 20 tours | MIT ou Apache-2.0. API en place, fonctions par défaut d'allocation et d'entropie désactivables, option `zeroize`. [Documentation de la bibliothèque](https://docs.rs/chacha20poly1305/0.11.0/chacha20poly1305/) · [Fonctionnalités](https://docs.rs/crate/chacha20poly1305/0.11.0/features) |
| HKDF, MAC et SHA-256 | `hkdf` 0.12.4, `hmac` 0.12.1, `sha2` 0.10.9 déjà verrouillé | RustCrypto, MIT ou Apache-2.0. Cette famille utilise les interfaces digest 0.10 et permet un cœur sans API de l'hôte. [HKDF et dépendances](https://docs.rs/crate/hkdf/0.12.4) · [HMAC](https://docs.rs/crate/hmac/0.12.1) |
| Phrase secrète | `argon2` 0.6.0 | MIT ou Apache-2.0. API à mémoire fournie par l'appelant, utilisable en environnement embarqué. Effacer également le tampon de blocs détenu par l'appelant. [Argon2](https://docs.rs/argon2/0.6.0/argon2/) · [API avec tampon](https://docs.rs/argon2/0.6.0/argon2/struct.Argon2.html#method.hash_password_into_with_memory) |
| Secrets et entropie | `zeroize` 1.9.1 et adaptateur hôte `getrandom` 0.4.3 déjà verrouillé pour ce dernier | MIT ou Apache-2.0. `zeroize` est utilisable en `no_std`. Le moteur reçoit une source d'entropie faillible. Un futur port NovaOS devra fournir un CSPRNG qualifié avant d'autoriser l'écriture chiffrée. [Zeroize](https://docs.rs/zeroize/1.9.1/zeroize/) · [Getrandom](https://docs.rs/getrandom/0.4.3/getrandom/) |

Ces dépendances sont des candidates. Le verrou P6 a été inspecté. Une compilation du graphe cryptographique final sur les deux cibles bare-metal, un contrôle des fonctionnalités transitives et des avis de sécurité des versions réellement verrouillées restent à effectuer avant livraison. Le futur cœur ne doit pas appeler de bibliothèque POSIX, de trousseau macOS ni de générateur de test en production.

## 8. Qualification et reproduction

Le test de bifurcation ci-dessous s'exécute depuis la racine du dépôt. Il construit les CLI du checkout P6 sur les cibles Darwin puis vérifie le comportement des deux branches sur des images temporaires ordinaires. Les numéros d'adresse attendus correspondent à la base auditée.

```sh
set -eu
for isa in aarch64 x86_64; do
    NOVAFS_TARGET="$isa-apple-darwin" novaFS/lab/run build --offline --bin novafs
done
python3 - <<'PY'
from pathlib import Path
from tempfile import TemporaryDirectory
from struct import unpack_from
import json
import subprocess

def allocation(image, contenu):
    bloc = 4096
    def entier(position):
        return unpack_from("<Q", image, position)[0]
    codes = [0, 1, 3, 2]
    copies = [image[2 * bloc + off] for off in (256, 768, 1280, 3840)]
    assert len(set(copies)) == 1
    catalogue = (3 + codes.index(copies[0])) * bloc
    assert image[catalogue:catalogue + 8] == b"NP6POOLC"
    generation = entier(catalogue + 32)
    racine = entier(catalogue + 128 + 72)
    positions = []
    for i in range(16):
        feuille = entier(racine * bloc + 32 + 40 * i)
        if feuille:
            for j in range(64):
                adresse = entier(feuille * bloc + 32 + 40 * j)
                if adresse and image[adresse * bloc:(adresse + 1) * bloc] == contenu:
                    positions.append((64 * i + j, adresse))
    assert len(positions) == 1
    return (generation, *positions[0])

for isa in ("aarch64", "x86_64"):
    cli = Path(f"novaFS/target/{isa}-apple-darwin/release/novafs").resolve()
    for secteur in (512, 4096):
        with TemporaryDirectory() as tmp:
            def action(nom, src, dst, *args):
                with (Path(tmp) / dst).open("wb") as sortie:
                    commande = [cli, nom, "1024", str(secteur), *args]
                    if src is None:
                        subprocess.run(commande, stdout=sortie, check=True)
                    else:
                        with (Path(tmp) / src).open("rb") as entree:
                            subprocess.run(commande, stdin=entree, stdout=sortie, check=True)
            action("pool-format", None, "vide", "11"*16, "p6")
            action("volume-create", "vide", "vol", "22"*16, "33"*16, "44"*16, "256", "none")
            action("file-create", "vol", "base", "1", "/f")
            origine = (Path(tmp) / "base").read_bytes()
            for octet in (0x41, 0x42):
                page = bytes([octet]) * 4096
                action("file-write", "base", "branche", "1", "/f", "0", page.hex())
                action("check-pool", "branche", "controle", "--full")
                assert json.loads((Path(tmp) / "controle").read_text())["statut"] == "coherent"
                action("file-read", "branche", "lecture", "1", "/f", "0", "4096")
                assert (Path(tmp) / "lecture").read_bytes() == page
                assert allocation((Path(tmp) / "branche").read_bytes(), page) == (4, 9, 22)
            assert (Path(tmp) / "base").read_bytes() == origine
            print(isa, secteur, "deux contenus validés à la génération 4, LBA 9, bloc 22")
PY
```

Le script de bifurcation ci-dessus a été exécuté sur les quatre combinaisons du tableau. Le lanceur de validation P6 a également été rejoué sur la base auditée, sous macOS 27.2. Les effectifs d'injection ci-dessous cumulent les deux tailles de secteur sur chaque architecture.

| Résultat réellement obtenu sur NovaFS **v0.9.8** | ARM64 natif | x86_64 sous Rosetta |
| --- | ---: | ---: |
| Tests Cargo standard / instrumentés réussis | 137 / 138 | 137 / 138 |
| Opérations différentielles / durée | 1 000 000 / 24,481 s | 1 000 000 / 40,190 s |
| P5, coupures / EIO / sélecteurs déchirés | 684 / 152 / 12 | 684 / 152 / 12 |
| P6, coupures / erreurs d'écriture sur la mutation compressée | 234 / 40 | 234 / 40 |
| P6, campagne aléatoire supplémentaire, coupures / échecs de flush | 48 / 48 | 48 / 48 |

Les suites standard et instrumentées se recouvrent. Le million différentiel et deux autres scénarios normalement ignorés ont été lancés explicitement. Les mesures antérieures d'échange d'images et d'arrêt de VM restent décrites dans la qualification P6. Le présent audit n'a lancé aucun test fonctionnel P7. Le débit, les latences, le pic mémoire du moteur chiffré et le coût de déverrouillage P7 sont **non mesurés**. [Validation de référence P6](novafs-p6-compression.md)

**Campagne à effectuer après arbitrage.** La première condition d'acceptation du futur moteur sera l'interopérabilité avec des vecteurs indépendants pour XChaCha, HKDF et Argon2id. Les tests de sécurité devront ensuite exercer les 30 points du cahier :

| Points | Oracle de validation sur ARM64 et Rosetta, pour les deux secteurs |
| --- | --- |
| 1–3 et 28–29 | Rejouer les suites P0–P6, le million différentiel, les coupures et E/S historiques, les images des anciennes révisions et les échanges binaires. L'ancien écrivain doit refuser P7. |
| 4–8 | Créer puis remonter un domaine neuf. Un mauvais secret ne livre aucun octet. Chaque emplacement correct déverrouille indépendamment. Le verrouillage invalide accès, cache et tickets. Vérifier l'effacement des buffers détenus et l'absence de clair dans l'image et les temporaires. |
| 9–14 | Modifier un octet du chiffré, du tag, du nonce ou des longueurs. Permuter unités, domaines, en-têtes, emplacements et références en recalculant les sommes publiques. Attendre un refus d'authenticité, aucune publication et aucun appel au décompresseur sur l'unité refusée. Tester aussi le verdict structurel sans clé. |
| 15–20 | Deux snapshots et deux clones du même domaine. Supprimer l'origine, lire tous les survivants et vérifier les octets après écritures partielles de 1 octet, 4 et 16 Kio. Vérifier LZ4 avant AEAD, repli brut des données incompressibles et maintien des droits et réservations. |
| 21–24 | Saturer avant publication, déchirer sélecteur et descripteurs, faire échouer écritures, F1, F2 et tirages d'entropie. Comparer le contenu récupéré à un modèle indépendant, y compris après reprise de deux copies divergentes. Observer les appels de chiffrement sur toute la trace, y compris ceux qui ont échoué avant publication. Tester les défaillances du mécanisme de nonces retenu. |
| 25–27 | Produire une archive, supprimer physiquement l'image source du laboratoire, restaurer avec la clé autorisée sur l'autre architecture et comparer les pages logiques. Retirer, réordonner, dupliquer et substituer des enregistrements et la clôture. Une destination préexistante doit conserver son reçu en cas d'échec. |
| 30 et performances | Même charge qu'en P6, avec pools non chiffrés, domaines chiffrés bruts et LZ4, contenus mixtes et incompressibles, snapshots et clones. Mesurer débits, p50/p95/p99/max, CPU AEAD, écritures et amplification, F1/F2, déverrouillage, vérification et export/restauration. Publier séparément tas + piles du moteur (cible 7 Mio), pic KDF hôte et RSS sur chaque ISA. |

Les campagnes déterministes de pannes pourront injecter un générateur réservé aux tests. Forcer un doublon permettra d'observer le comportement du profil choisi. Une longue série sans doublon ne démontre pas une unicité stricte. Une clé candidate ou un chiffré d'essai ne doit jamais quitter les fixtures jetables.

Pour rejouer les suites P6 de référence, depuis la racine :

```sh
set -eu
resultats=$(mktemp -d)
for isa in aarch64 x86_64; do
    python3 novaFS/lab/p6-validation.py "$isa" --sortie "$resultats/$isa.json"
done
```
