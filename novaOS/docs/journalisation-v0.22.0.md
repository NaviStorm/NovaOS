# Journalisation persistante des services — NovaOS v0.22.0, phase 2

## Révisions et périmètre

La phase 1 a été intégrée par fusion de `7b9a4b56eed67889c862928bc5a1e63b13a45657` sur `38d4c887c68a677e48228a8b33882300ba523ec0`. Le commit de fusion publié sur `origin/main` est `ac4cfb76c0a4c4789283b1776bb93491c511894a`. Son arbre et celui du candidat qualifié ont la même empreinte `6e3a9c599f2a69e1c9db4c9b0ed9ff8cf4820e00`. La phase 2 se qualifie sur `codex/journalisation-0220-phase2`. Les bannières du noyau indiquent 0.20.0, le SDK Rust 0.2.0, NovaFS 0.9.13 P9 et la dernière image distribuée reste celle de v0.17.0. Les ISO du banc servent uniquement au démarrage des essais. [Phase 1](services-configuration-v0.22.0.md)

## Amorçage et disponibilité

1. Le lanceur relève deux horodatages monotones en mémoire, à l’entrée de la session de stockage et à l’obtention du droit bloc. Le relais et le pilote démarrent, NovaFS monte P9 et crée le `tmp` du gestionnaire.
2. Le gestionnaire entre avec son catalogue et lit la configuration. Il conserve BOOT et ses jalons dans une file volatile. Le temps de BOOT est celui du premier jalon. Les deux données correspondent à l’ouverture du bloc et à l’observation du gestionnaire après création du `tmp`. Les erreurs de configuration produisent des événements distincts.
3. Le service 16 est un ELF utilisateur supervisé. Il reçoit du lanceur sa paire privée vers P9. Son annonce suit l’acquittement de son propre événement ONLINE. Le gestionnaire peut alors transférer les événements précoces. La séquence globale ordonne les publications, y compris lorsqu’un événement précoce porte un horodatage plus ancien que celui d’ONLINE.
4. L’instance courante du service 9 émet son annonce sur son endpoint authentifié. Le gestionnaire confirme READY seulement après la vraie réponse INFO `NVFS`. Le service 10 peut ensuite satisfaire sa dépendance. L’ordinal de l’événement, le BootID, les identités du sujet et de l’émetteur permettent de distinguer plusieurs incarnations au cours du même démarrage.

La file du gestionnaire contient 32 trames de 128 octets. Une seule publication attend son acquittement. Le gestionnaire conserve en file les demandes qui n’ont pas encore été admises et comptabilise tout débordement. Une remise suivie d’une perte de réponse incrémente le compteur des issues incertaines. Les opérations admises quittent la file une fois. La supervision n’attend aucune écriture du journal. Le drainage volontaire lui accorde jusqu’à 1 500 ms. Un arrêt plus tôt, une saturation ou une panne peut perdre des événements encore volatils, notamment le dernier événement d’arrêt. `service status 16` expose pertes, issues incertaines et dernier statut du gestionnaire indépendamment du démon. [Publication bornée](../user/services/journal_publisher.rs) · [Supervision](../user/service_manager.rs)

Le `tmp`, l’accès minimal P9 et le traitement des commandes ne dépendent pas du service 16. Le service utilise du stockage système privé dès son lancement. Deux racines imputées au relais bloc ont zéro droit d’usage. Le noyau dérive des droits directionnels pour le client et le journal et les retire à la fin de l’incarnation. Cette répartition conserve les deux emplacements nécessaires au lancement par `nsh` de commandes isolées comme `cat`. [Raccordement et retrait](../kernel/src/sched/process/ipc/service.rs)

## Autorités et conservation

En session ordinaire, le lecteur peut demander les événements de sa propre instance. Le profil explicite `journal-admin` dispose aussi de la lecture système, de l’émission d’une notice numérique et de trois droits distincts pour la rétention, la rotation et la purge. Les décisions du gestionnaire et les annonces d’un producteur conservent des provenances distinctes. Le journal n’enregistre aucun texte libre provenant d’un processus. Le catalogue reste l’autorité sur les droits exacts des services. La configuration persistante ne sait pas en retirer individuellement. [Protocole et sécurité](journal-protocole-v0.22.0.md) · [Commandes nsh](journal-nsh-v0.22.0.md)

Chaque événement occupe 96 octets. La rétention maximale est de huit segments de seize événements, soit 12 288 octets utiles. Le magasin publie ensemble le contenu et les métadonnées par CoW P9 avec les barrières F1 et F2. La première installation publie un objet préparé sous un nom privé. Les append, rotations et purges ultérieurs effectuent chacun une transaction effective. Une lecture, une répétition identifiée dans la fenêtre retenue ou une décision administrative sans changement n’entraînent pas de nouvelle publication. La réserve préserve au minimum 16 blocs P9 pour les autres usages, au-delà de la réserve du moteur. [Persistance, quota et récupération](journal-stockage-v0.22.0.md)

## Reproduction des pannes

Depuis le worktree, configurer `TMPDIR` et `NOVAOS_TMPDIR` selon les consignes du dépôt. Construire au préalable l’outil hôte NovaFS et préparer un répertoire neuf de preuves durables. Les déclenchements du banc n’existent que dans le binaire de laboratoire. Le noyau ordinaire, son ABI et les droits du profil de maintenance restent identiques pour les appels du client.

```sh
cd novaOS
RUSTC="$(rustup which --toolchain stable rustc)" rustup run stable cargo build \
  --release --locked --offline --features journal-pannes-lab \
  --target aarch64-unknown-none-softfloat --target-dir build/target-journal-pannes
python3 -B tests/journal_pannes.py aarch64 4 512 "$PREUVES/pannes-arm"
```

Pour x86_64, utiliser la cible `x86_64-unknown-none` et l’argument `x86_64` du banc. Chaque expérience utilise un disque isolé, conserve les journaux série, vérifie l’image P9 indépendamment et la relit après un arrêt forcé de QEMU. Les parcours ordinaires et les mesures de rotation utilisent les ELF sans sondes. [Parcours des services 9 et 10](../tests/journalisation.py) · [Coupures du démon](../tests/journal_pannes.py) · [Réserve atteinte sur VirtIO](../tests/journal_saturation.py)

## Matrice de qualification

Le 15 novembre 2026, les deux ELF ordinaires ont été exécutés dans QEMU 11.1.3, TCG, 256 Mio, avec stockage VirtIO sur des images brutes P9. Chaque cellule comprend une session ordinaire aux droits restreints puis trois sessions administratives sur un même volume. Les latences murales en ms sont celles d’une émission acquittée dans chacune des trois sessions. Elles incluent les commandes de script et le transport série. Les SHA-256 des ELF et des rapports ainsi que les valeurs individuelles en nanosecondes sont conservés avec les BootID. [Résultats individuels](validation/journalisation-v0.22.0.json)

| Invité | vCPU | Secteur en octets | Désactivation / attente / réactivation, ms |
|---|---:|---:|---:|
| aarch64 | 1 | 512 | 50,68 / 55,07 / 65,07 |
| aarch64 | 1 | 4096 | 50,19 / 52,95 / 69,31 |
| aarch64 | 2 | 512 | 51,42 / 56,43 / 56,35 |
| aarch64 | 2 | 4096 | 54,15 / 49,80 / 56,13 |
| aarch64 | 4 | 512 | 50,97 / 52,11 / 65,16 |
| aarch64 | 4 | 4096 | 51,55 / 52,07 / 57,16 |
| x86_64 | 1 | 512 | 48,69 / 40,28 / 46,61 |
| x86_64 | 1 | 4096 | 51,98 / 45,92 / 42,45 |
| x86_64 | 2 | 512 | 49,55 / 41,31 / 42,82 |
| x86_64 | 2 | 4096 | 47,15 / 49,39 / 47,71 |
| x86_64 | 4 | 512 | 56,03 / 44,64 / 50,54 |
| x86_64 | 4 | 4096 | 42,26 / 44,46 / 43,91 |

Les douze cellules ont réussi. Les deux parcours de saturation supplémentaires portent la matrice à 50 démarrages. Deux autres rejeux ciblent la pagination après rotation. L’oracle a vérifié les séries, les disques compressés relus, les ISO et les ELF extraits des 50 ISO. Les 36 BootID des sessions d’administration sont distincts. Par exemple, en AArch64 à un vCPU et secteurs de 512 octets, les séquences 13 et 17 attestent les READY des services 9 et 10. La séquence 18 consigne la désactivation de 9. Au démarrage suivant, 23 consigne l’attente de 10 et 32 la réactivation de 9. Au troisième démarrage, 47 et 51 attestent les deux READY. Ce troisième terminal a relu les événements des trois démarrages.

| Épreuve | Profils | Résultat observé |
|---|---|---|
| Émission par le gestionnaire et les services, classement par identité, service, gravité et BootID, consultation et suivi | 12 cellules QEMU | nsh retrouve les trois historiques. En session ordinaire, la lecture système, l’administration et l’émission sont refusées. Les fichiers publics et INFO 9 restent disponibles. |
| Rotation automatique et manuelle, lecture inverse, rétention et purge | Deux invités, 1 vCPU, 512 octets | Après 33 notices et rétention de deux segments, 17 notices occupent 1 632 des 3 072 octets autorisés. La lecture inverse rend la totalité d’une page courte après éviction. La purge est relue et les fichiers publics restent accessibles. |
| Faute du processus après remise avant append ou après commit avant réponse | Deux invités avec 1, 2 et 4 vCPU, 6 cellules du noyau de laboratoire | 24 cas sur volumes distincts, chacun suivi d’un SIGKILL de QEMU et d’un remontage indépendant. Les deux points antérieurs à l’append laissent zéro copie. Le point post-commit en laisse une. La notice confirmée auparavant reste présente. Le client rapporte UNCERTAIN lorsque la réponse manque. |
| Journal arrêté, redémarré, panne de ses écritures ou de sa rotation | Mêmes 6 cellules | Entre deux incarnations, le canal fermé refuse la demande avant remise. Rotation injectée en erreur 26, puis stockage du journal indisponible en erreur 26. Le gestionnaire continue de superviser et d’administrer les services. Les avis de retrait et de redémarrage sont relus. |
| Espace P9 réellement insuffisant pour inaugurer le journal | AArch64/512 et x86_64/4096, 2 vCPU | Service 16 en échec. Une sentinelle de 1 245 184 octets, la réserve de maintenance de 128 blocs et les requêtes NVFS survivent au stop/start de 9. La copie de contrôle renvoie NoSpace, SHA-256 inchangé. |
| Configuration valide en syntaxe, droits excessifs | AArch64/512 et x86_64/4096, 2 vCPU | Refus du droit et événement CONFIG_ERROR de code 21 sur le BootID courant. Le service 10 attend 9 et cat lit P9. L’outil hôte retrouve les mêmes objets de configuration. |
| Usurpation, révocation en attente, identité périmée, lecture concurrente et acquittement perdu | Darwin ARM64 natif et x86_64 sous Rosetta | 8 tests par architecture de l’automate avec IPC simulé, dont les 64 masques. Quatre tests du transport et un test de saturation du gestionnaire par architecture. Les lecteurs en attente sont réveillés après publication durable. Le gestionnaire comptabilise deux issues incertaines sans répétition. |
| Coupure pendant inauguration, append, rotation ou purge | Moteur P9 réel, périphérique simulé, deux architectures et deux géométries | Par combinaison, 138 points de coupure, 24 échecs de FLUSH et 48 écritures partiellement persistées. Deux préfixes d’ancre de 96 octets imposent le refus sécurisé du montage normal. Les générations retenues par le montage sont entières. |
| Compatibilité historique | ABI et bancs QEMU | 9 contrôles ABI, 48 sélecteurs, en-têtes C/C++ et dispatch cohérents. Sur P8, quatre démarrages par architecture, lecture sans variation de génération et remontages G43/G44. La sonde historique de perte du gestionnaire constate le retrait du canal sur les deux architectures. |

## Mesures et limites

| 33 émissions séquentielles, 1 vCPU, secteurs de 512 | AArch64 | x86_64 |
|---|---:|---:|
| Latence p50 | 51,61 ms | 43,98 ms |
| Latence p95 | 106,49 ms | 48,97 ms |
| p99 et maximum | 108,57 ms | 75,72 ms |
| Débit depuis nsh | 16,01 événements/s | 22,17 événements/s |
| Rotation confirmée, un échantillon | 92,12 ms | 74,96 ms |

Les valeurs incluent la saisie, l’exécution et la sortie série. D’autres invités de qualification tournaient sur le Mac. Le débit du moteur seul avec SimDevice est documenté séparément. Le cache de lecture du démon occupe 3 072 octets. Son processus compte 5 pages privées hors pile et 64 pages de pile, soit **276 Kio** mesurés au lancement et au retrait sur les deux architectures. Le pic de pile effectivement utilisée reste inconnu. Les 38 arrêts de maintenance mesurés restituent les pages utilisateur, piles, objets IPC et processus aux compteurs initiaux. Le pic du système complet varie de 930 à 1 141 cadres sur AArch64 et de 935 à 1 148 sur x86_64. Les différences de cadres libres à la collecte correspondent aux tables de pages et au DMA comptabilisés par le noyau. [Mesures détaillées](validation/journalisation-v0.22.0.json)

La coupure électrique physique et la restauration de tout le support restent hors de la preuve. Un refus de montage P9 empêche l’accès au journal sur ce volume. Aucun mécanisme externe n’atteste sa fraîcheur. Le diagnostic d’un amorçage où P9 échoue repose sur les sorties volatiles. Le stockage complet et le journal restent sujets au même risque de rollback physique.

La qualification soutient l’examen de la phase 2 par la MOA/MOE. Le retrait individuel de droits par configuration reste soumis à un arbitrage distinct. Le candidat serveur v0.30.0 devra encore qualifier les scénarios distribués S07 et S10 avec ses autorités définitives. [Matrice serveur](reconciliation-v0300/acceptation-serveur.md)

## Traçabilité des preuves

Les journaux série, ISO de qualification, traces de stockage, images comprimées et noyaux relus sont conservés dans le dossier durable `OS-worktrees/preuves-journalisation-0220-phase2/`. Le bilan des 50 démarrages se trouve dans `cli/bilan-livraison.json`, SHA-256 `561b3850f194075887ce54cf99ec42452668b2a6794bb134c99cac7918b59441`. Les six parcours de panne des binaires instrumentés figurent dans les dossiers `livraison-pannes-*`, les essais de saturation et de refus de configuration dans `livraison-saturation-*` et `livraison-config-*`. Les essais exploratoires y sont aussi conservés avec leurs échecs initiaux. Les preuves d’acceptation correspondent aux binaires identifiés dans les résultats. [Matrice et empreintes des candidats](validation/journalisation-v0.22.0.json)

Une seconde copie physique se trouve hors du worktree dans `OS-worktrees/archives-journalisation-0220-phase2/`, sur le même Mac. Les 1724 fichiers du conteneur `preuves-journalisation-0220-phase2.tar.gz` ont été décompressés, relus et comparés individuellement par SHA-256. Empreinte de l’archive complète : `60baf4334d844a99d0b00c8a4868dcd93305042cde6663a6d310d5f6c857101b`. Le manifeste `SHA256SUMS` et `verification.json` se trouvent à côté de l’archive.
