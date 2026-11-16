# NovaAuth v0.24.0 — Phase 3, qualification du facteur de possession

Le raccord de qualification authentifie deux comptes dans de vrais processus NovaOS. Le lot des permissions applicatives reste ouvert. La version distribuée conserve son catalogue ordinaire.

## Références et reproduction

| Objet | Révision |
| --- | --- |
| Base intégrant le registre P9 de phase 2 | `2e3188dee6eaef859becca3919bd0d53a20674bb` |
| Révision initiale du registre | `2d1a8d61dc036f6c71cc3994c247545dc548951c` |
| Contrat des cinq lots et onze recettes | `66a34d67d17078cc6d94934f27cc2e57a9d3833b` |
| Code exécuté dans la campagne finale | `8821247d158073c6c7b3d61874bd9f75aed5ec25` |

Depuis la racine d'un worktree à cette révision, appliquer les règles de `TMPDIR` du projet. Choisir un répertoire de compilation neuf. L'export authentifie les fichiers du raccord par leur identifiant Git avant application. Les fichiers du noyau ordinaire qui définissent le dispatch et la liste fermée restent ceux de la base. Une compilation ordinaire ne référence aucun des deux ELF.

```sh
python3 novaOS/user/novaauth/service/profile.py "$NOVAOS_TMPDIR/qualif-auth"
cd "$NOVAOS_TMPDIR/qualif-auth/novaOS"
export RUSTC="$(rustup which --toolchain stable rustc)"
export NOVA_ARCH=aarch64
target=aarch64-unknown-none-softfloat
rustup run stable cargo build --locked --offline --release -p novaos --target "$target" --features novaauth-sessions-lab
NOVA_KERNEL_ELF="$PWD/target/$target/release/novaos" sh scripts/iso.sh novaauth-sessions "$NOVAOS_TMPDIR/auth.iso"
python3 user/novaauth/service/qemu.py "$NOVA_ARCH" 512 "$NOVAOS_TMPDIR/auth.iso" "$PREUVES/genese/recette-aarch64-512" "$SORTIE"
```

`PREUVES` désigne le dossier de preuves décompressé et `SORTIE` un chemin neuf. Pour x86_64 employer `x86_64-unknown-none` et l'image initiale correspondante. Le banc impose deux vCPU, une image jetable NovaFS de 512 pages et un fichier d'autorité distinct de 12 pages. Les deux étendues sont présentées au pilote via un descripteur VMDK. Le scénario ne constitue pas une qualification de deux périphériques matériels indépendants.

## Authentification et autorité

Le lanceur peut amorcer un domaine vierge. Le registre de phase 2 décide de manière durable. Un canal distinct permet au vérificateur d'obtenir le titulaire actif, la référence du facteur et l'empreinte de tête. Pour chaque mutation administrative le registre contrôle à nouveau le compte, son rôle et la tête attendue. Une réponse perdue après remise laisse une issue `UNCERTAIN`. L'identifiant de requête permet la relecture d'une décision durable. Le client de la recette relit la décision d'enrôlement.

Le vérificateur et les deux clients occupent trois espaces d'adressage. Le noyau fixe les incarnations et les extrémités IPC. Le masque des capacités de service des nouveaux clients vaut zéro. Les essais dans chaque client exigent le refus de `tmp::CAP` et du contrôle d'horloge du gestionnaire. Les mutations sont accessibles au seul administrateur authentifié par la voie privée du vérificateur. L'autorisation d'écrire un dossier reste distincte.

Chaque client crée sa propre clé Ed25519 éphémère. Le lanceur reçoit la référence publique. Le vérificateur reçoit la clé publique et la signature. La référence enregistrée contient 128 bits du condensat de domaine et de clé. La transcription signée lie le domaine, le compte, le facteur, la tête du registre, l'incarnation du vérificateur et du client, la demande de session, le jeton, le nonce et l'expiration. Le serveur consomme le défi avant de vérifier la réponse. Il relit le registre avant d'ouvrir la session. La boucle sérialise les admissions. La session est attachée au canal et au titulaire. La fermeture ou la perte du canal retire les sessions correspondantes. Les contrôles au registre empêchent l'usage ultérieur du facteur d'un compte désactivé ou supprimé.

Le plafond est de huit défis et seize sessions. Les échéances sont de 1 500 ms et 60 000 ms respectivement. La limitation des tentatives utilise le canal, le facteur et une pénalité globale. Les tables bornées, la saturation, l'expiration et la restitution des emplacements ont des tests hôte. Les événements série consignent les décisions et les délais. Le raccord au journal persistant de sécurité et la garde d'un facteur sur matériel de production restent à qualifier. L'interface `Factor` sépare le vérificateur du mécanisme de preuve. Un facteur Argon2id nécessiterait un canal de saisie protégé et un protocole adapté. Les clés du banc sont créées en mémoire à chaque lancement. Le redémarrage sur un domaine déjà initialisé démontre le refus du réenrôlement. La reconnexion d'un client doté d'une clé conservée demeure à tester.

## Résultats dans les invités

QEMU 11.1.3, deux vCPU. La durée hôte couvre démarrage, opérations des trois ELF, délais volontaires et arrêt. Les quatre mesures invité par ligne couvrent la remise d'une réponse valide jusqu'à la création de session et l'émission du marqueur, avec les accès réels au registre. La signature côté client et l'émission du défi précèdent cette mesure.

| Invité | Secteur P9 | Durée hôte | Vérifications des quatre réponses valides, ms invité | Pages privées au lancement, serveur / chaque client |
| --- | ---: | ---: | --- | --- |
| AArch64 | 512 | 49,67 s | 1396, 1592, 1464, 1340 | 18 / 17 |
| AArch64 | 4096 | 36,00 s | 908, 896, 924, 848 | 18 / 17 |
| x86_64 | 512 | 38,69 s | 1404, 1428, 1436, 1284 | 21 / 19 |
| x86_64 | 4096 | 28,21 s | 980, 980, 980, 976 | 21 / 19 |

Chaque page mesure 4 Kio. Le noyau rapporte séparément 64 pages de pile par processus. Les instantanés de lancement ne mesurent pas le pic résident en charge. Les trois enfants sont collectés après fermeture de leurs espaces et de leurs instances. La qualification spécifique ne mesure pas un bilan global de chaque cadre physique après ce scénario.

Les quatre campagnes ont observé trois sessions valides simultanément, la fermeture indépendante d'une session, le refus d'un emprunt entre canaux, d'une signature altérée, d'un défi expiré et d'une preuve rejouée. L'administrateur renomme le compte, retire son groupe, le désactive, le supprime puis recrée son nom. Le compte recréé doit présenter un nouveau facteur. Une relecture indépendante identifie la génération 7, la tombe de l'ancien compte et l'identité distincte du nouveau.

Douze démarrages à froid du même candidat couvrent trois états sur chacune des quatre configurations. Une nouvelle tentative d'enrôlement sur domaine existant est refusée. La présentation du seul ancien volume avec l'autorité actuelle refuse l'activation. L'effacement de l'autorité de laboratoire refuse également l'activation. Dans ces douze cas le banc contrôle l'absence d'authentification, la conservation des octets de l'autorité après démarrage, la collecte des enfants et l'arrêt ordonné. Le lecteur indépendant ouvre le domaine valide ou constate `StaleVolume` ou `NotEnrolled` selon le cas. La coupure physique, le rollback simultané du volume et de son autorité logicielle, et l'authentification de production restent hors qualification.

## Recettes d'acceptation

| N° | État et portée des preuves |
| ---: | --- |
| 1 | Réussie dans les quatre invités pour trois sessions, deux comptes et les refus cryptographiques. |
| 2 | Ouverte. L'origine d'un handle VFS effectivement autorisé sur D doit être convenue. |
| 3 | Partielle. L'emprunt d'une session d'un autre canal est refusé. L'isolation d'une délégation VFS sur D reste à tester. |
| 4 | Partielle. Fermeture indépendante et restitution vérifiées. Une opération VFS pendante et ses descendants restent à exercer. |
| 5 | Ouverte. Politique de l'application arrêtée, identité effective d'une nouvelle application et attribution à chaud à démontrer avec un effecteur. |
| 6 | Ouverte. Le retrait du facteur et de la session a été exercé. La linéarisation d'un retrait en cours d'E/S sur D reste à démontrer. |
| 7 | Ouverte. La chaîne de sous-délégations VFS et son témoin indépendant nécessitent D. |
| 8 | Ouverte pour l'effecteur. Les archives du registre couvrent la perte de réponse et la recherche de décision. L'issue d'une écriture sur D n'en découle pas. |
| 9 | Partielle. Retrait du groupe publié, compte désactivé en session, ancien facteur refusé, compte administrateur toujours opérationnel. Retrait d'un droit VFS dérivé du groupe à mesurer. |
| 10 | Partielle. Identité distincte, anciens facteur et session refusés, tombe relue. L'appartenance initiale du nouveau compte est explicitement attribuée lors de la création. Les anciens handles de fichiers privés, ACL et caches restent à contrôler de bout en bout. |
| 11 | Témoin de registre exécuté en ARM64 natif et x86_64 sous Rosetta pour les deux tailles de secteur. Il réévalue titulaire et groupe avec un mandat sans capacité de session. Retrait du groupe, suppression et réutilisation du nom entraînent le refus. La déconnexion réelle suivie de l'exécution d'un processus planifié reste à qualifier. |

Le lot permissions requiert une autorité initiale sur D, un effecteur vérifiant chaque droit et sa filiation, une identité d'installation attestée et un contrôle à la remise de toute opération. L'alternative soumise à la MOE est le provisionnement explicite d'un dossier de laboratoire ou le transfert par un propriétaire détenant déjà le handle. Les révocations simultanées, l'acquittement tardif après redémarrage du service et l'exécution en SSH restent ouverts. Aucune de ces capacités ne doit être déduite de la seule authentification du compte.

## Compatibilité et non-régression

- Les deux noyaux ordinaires construits sur le candidat sont identiques octet par octet aux binaires de la base. Pour chaque architecture, les octets complets du vérificateur et du client ELF sont présents dans le noyau de qualification et absents du noyau ordinaire. La construction comporte un export Git séparé, une feature explicite et une sélection du scénario au démarrage.
- Neuf contrôles du registre ABI ont réussi. Aucun changement sous `novaOS/abi` ni dans le moteur `novaFS/`. L'ABI native v1 conserve ses 48 sélecteurs et le format est NovaFS v0.9.13 P9.
- `services.py` échouait sur la base avant toute modification du banc. Le premier démarrage ordinaire ajoutait le journal privé. L'oracle décode l'objet, vérifie son intégrité et son unicité, puis compare toutes les autres entrées à l'inventaire initial de 37 entrées. Les douze combinaisons architecture/secteur/vCPU ont parcouru toute la suite, soit 52 démarrages.
- Suites IPC et cycle de vie des processus exécutées sur les deux architectures. Les pipes `cat | wc` et `cat | head` ont été exercés dans les quatre combinaisons architecture/secteur. Au premier essai de `cat | head` x86_64/4096, le comparateur du cas UTF-8 tronqué a reçu aussi l'écho de commande. Le rejeu intégral a réussi. L'origine de l'intermittence demeure ouverte. L'archive conserve les deux essais.
- La qualification porte sur QEMU et sur des oracles hôte natifs/Rosetta. Les essais de stockage de phase 2 repris dans le dossier `phase2-registre` ont été vérifiés contre leur manifeste d'origine. La distribution UTM et le matériel physique restent hors de cette campagne.

## Preuves et suite de la décision

La copie complète est dans le dossier frère durable `archives-novaauth-phase3-0240`, fichier `qualification-8821247.tar.gz`. Le manifeste recense chaque fichier par chemin relatif. Les octets de chaque membre décompressé ont été relus et comparés au fichier original. L'extrait versionné regroupe les journaux de qualification et le manifeste complet. [Empreintes et nombres de fichiers](validation/novaauth-phase3-0240.json) · [Extrait vérifié](validation/novaauth-phase3-0240.tar.gz)

La branche sert à la poursuite de la qualification. L'acceptation d'ensemble dépend des recettes restées ouvertes et de la décision sur l'autorité du dossier D.
