# NovaAuth R2 — Stabilisation du témoin x86_64/P9-4096

22 novembre 2026. Base publiée : `2cdfbd502e57b19e2fad8ed80cc61c9f782c5d64`.
Verdict : **deux exécutions indépendantes réussies du témoin corrigé**, dans le
seul profil QEMU x86_64/P9-4096. R2 reste incomplet ; aucune qualification de
production, d'un autre profil ou des neuf scénarios complémentaires n'en découle.

## Causes établies et corrections

Le premier essai de ce mandat conserve le défaut TLS, avec des sondes qui
préservent l'erreur avant sa conversion en statut 12. Le signataire PID 8 reçoit
`Io(TimedOut)` à l'étape 3, opération CIPHERTEXT (19), séquence 19. Aucun statut
IPC supplémentaire n'est enregistré (`native=0` signifie absence de statut IPC,
pas succès TLS). Les deux fragments précédents occupent respectivement
9 136–9 800 ms et 9 800–10 456 ms invités. Le fragment suivant commence à
10 456 ms, pour une échéance absolue à 10 852 ms. Le nettoyage finit à 11 036 ms.
Le dépassement final de 184 ms inclut le nettoyage ; il n'est pas une extension
autorisée du dialogue. Le refus sûr et la quarantaine sont conservés.

Le chemin `authority_cell` employait `Block::transfer` pour chaque secteur de
4 096 octets, soit 43 trames PIECE en plus de BEGIN/END, y compris lors des
lectures successives d'une cellule de transport. Les traitements hôtes des
fragments correspondants durent environ 0,35–0,41 ms, sur l'horloge hôte ; ce
constat ne soustrait aucun horodatage hôte d'un horodatage invité.

Le générateur du seul export `--administration-ticket-vivant` réutilise désormais
`read_page`/`write_page`, avec le handle de page **déjà délégué** en config(10),
uniquement en géométrie 4096 et si ce handle existe. Ces méthodes conservent
FEATURE_PAGE négocié et leur repli historique. Les contrôles de mode, taille,
volume de 514 pages, pages réservées 512/513, emprunt exclusif et erreurs natives
restent en place. Aucun cache, cadre, handle ou privilège supplémentaire n'est
créé par cette sélection. Le chemin 512 reste inchangé. Le passage aux pages
réduit le dialogue TLS complet à 2 900 puis 2 816 ms dans les deux essais corrigés,
sans changer la limite de 10 secondes ni aucun contrôle cryptographique.

Le défaut d'ordonnancement est distinct : l'ancien banc attendait Bob avant de
lancer l'ELF de confirmation d'Alice. Ce chargement indépendant occupait alors
la fenêtre du ticket. Le nouveau rendez-vous utilise la deuxième préparation
identique, émise par l'ELF de confirmation **déjà chargé**, pour déclencher Bob.
L'opération, son empreinte et la session d'Alice doivent correspondre à la première
préparation. La réponse de préparation attend que Bob ait reçu son ticket puis
fait contrôler sa session existante via NSHLIVE. Aucun bail n'est renouvelé.
L'attente est bornée à 20 secondes (borne cliente existante), réduite à l'échéance
de la capacité administrative. Un changement de route détruit l'attente ; une
expiration ou une session révoquée produit un refus. La véritable confirmation
et tous les contrôles administratifs côté serveur restent exécutés ensuite.

Le code 12 ancien de Bob au commit `bf66f21` n'avait pas de sous-statut : son
chemin exact n'est pas rétrospectivement démontré. Le présent diagnostic prouve
le refus TLS instrumenté et valide l'ordonnancement corrigé ; il ne réécrit pas
les anciennes preuves pour attribuer artificiellement toute sortie 12 au délai.

## Sources testées

| Commit | Contenu |
|---|---|
| `f8ca300cff2c18e983e8fa2b8846d8b9b03029d0` | Rendez-vous privé QEMU et diagnostic TLS ; premier essai en échec TLS |
| `48fd525746e5b6dfceb3fbfe7cb8767d068e7614` | Transfert PAGE existant et test de route ; sources des deux invités réussis |
| `20a9decfb3865132bd96cb441c2c9b92dc86721d` | Test hôte du rendez-vous ; banc utilisé pour les deux succès |

Entre les deux derniers commits, seul le nouveau test hôte change. Les deux
succès utilisent exactement les mêmes SHA-256 ISO, noyau et registre : aucune
recompilation entre eux. Chaque essai reçoit une copie neuve de la même genèse
P9 et de l'ancien disque local d'autorité, ainsi qu'une autorité indépendante
nouvelle pour ce parcours. L'autorité ne revient jamais en arrière à l'intérieur
d'un parcours.

QEMU 11.1.3, TCG, deux vCPU, 256 Mio, `-nic none`, P9/4096. Le profil ISO
`novaauth-sessions`, le noyau réellement embarqué et le registre inclus ont été
vérifiés avant exécution. Les commandes intégrales, versions et empreintes sont
archivées. L'avertissement historique rust-objcopy/libLLVM est conservé ; les deux
constructions Cargo réussissent.

## Matrice réellement exécutée

| Contrôle | Essai 01 | Essai 02 | Essai 03 |
|---|---|---|---|
| Récupération TLS | Échec sûr Io(TimedOut) | Réussite | Réussite |
| Deux sessions distinctes | Non exécuté | Réussite | Réussite |
| Ticket Bob valide, non consommé | Non exécuté | Réussite | Réussite |
| Ancien ticket : REVOKED avant expiration | Non exécuté | Réussite | Réussite |
| Aucun enfant de Bob | Non exécuté | Vérifié | Vérifié |
| Reçu durable RETIREE | Non exécuté | Vérifié et signature relue | Vérifié et signature relue |
| Préparation, confirmation, consultation par Alice | Non exécuté | Trois statuts 0 | Trois statuts 0 |
| Drainage final | Non qualifié | Réussite | Réussite |

Les réponses Alice ont respectivement 134, 70 et 70 octets de stdout, comparés
exactement ; stderr est vide. Les PID enfants sont 21, 22 et 23, tous rattachés
à Alice (PID 5). Bob (PID 11) ne crée aucun enfant. Sa fermeture de session est
acquittée ; son processus témoin attend ensuite le retrait normal de la famille
supervisée (125). Ce retrait n'est pas présenté comme une sortie normale 0.

La relecture indépendante des deux bases d'autorité, en lecture seule, vérifie
une seule mutation administrative, le masque d'acquittement 7, l'état RETIREE,
la signature Ed25519 du reçu terminal et l'absence d'admission d'enfant de Bob.
L'invité a lui-même annoncé la vérification du reçu terminal. Ce contrôle hôte
complète les preuves invitées, sans les remplacer.

49 tests hôtes finaux réussissent (3,331 s), dont deux nouveaux contrôles exécutant
le corps Rust de la route de transport et du rendez-vous avec des effecteurs
contrôlés : chemin 512 conservé, limites et erreurs, octets lus/écrits, événement
absent, mauvaise session, session Alice révoquée, expiration et réponse unique.
Ces tests ne sont pas comptés comme qualification QEMU.

## Chronologie monotone invitée

Valeurs en millisecondes de l'horloge invitée ; les ticks noyau sont convertis
avec 250 Hz. Les horloges des deux invités restent séparées.

| Événement | Essai 02 | Essai 03 |
|---|---:|---:|
| Début / fin TLS | 844 / 3 744 | 844 / 3 660 |
| Échéance TLS | 10 844 | 10 844 |
| Défi / vérification Alice | 5 524 / 5 792 | 5 436 / 5 684 |
| Fin ouverture / échéance bail Alice | 6 328 / 66 064 | 6 160 / 65 932 |
| Scellement ELF confirmation Alice | 12 128 | 11 756 |
| Défi / vérification Bob | 15 872 / 16 144 | 15 320 / 15 592 |
| Fin ouverture / échéance bail Bob | 16 724 / 76 440 | 16 128 / 75 864 |
| Entrée Confirm | 20 464 | 19 864 |
| Présentation ancien ticket Bob, REVOKED | 20 772 | 20 156 |
| Expiration naturelle du ticket | 24 044 | 23 420 |
| Marge restante au refus | **3 272** | **3 264** |
| Publication : début échange / réponse réussie | 21 048 / 27 328 | 20 416 / 26 652 |

Le refus local précède l'acquittement durable terminal ; le rapport ne confond
pas ces garanties. La réponse réussie de Confirm exige les effecteurs et le
reçu durable. Les marqueurs de scellement ne mesurent pas à eux seuls toute la
résolution et la lecture P9 de l'ELF ; aucune durée complète de chargement n'est
déduite de leur intervalle parfois inférieur à un tick.

## Ressources et compatibilité

Pic observé : quatre racines au maximum par acteur, dont Alice deux, Bob quatre,
NovaFS quatre, vérificateur quatre, registre quatre et signataire quatre. Le
contrôle de cinquième racine refusée reste positif. Les tableaux par PID sont
conservés dans la synthèse.

Avant/après le dialogue initial, le signataire reste à 4 racines, 6 handles,
0 attente, 68 pages et 64 pages de pile ; le registre à 4 racines, 7 handles,
0 attente, 145 pages et 64 pages de pile. Ce sont des points de mesure, pas une
preuve de maximum de toutes les allocations intermédiaires. Le drainage final
ramène endpoints, nœuds, espaces et cadres IPC à zéro ; les lignes de restitution
des deux shells donnent racines, handles, attentes, pages, pile et threads à zéro.

Le diff depuis la base est vide pour `novaOS/kernel`, `novaOS/abi`, `novaFS` et
`components`. Les modifications invitées sont appliquées seulement par le
générateur ticket-vivant. NXEXEC01, l'ABI v1, les 48 sélecteurs, les capacités,
les baux et les noyaux ordinaires ne sont pas modifiés. Les neuf contrôles ABI,
les quatre ELF historiques, les autres profils QEMU et les autres recettes R2
ne sont **pas rejoués** dans ce mandat ciblé ; aucune réussite nouvelle n'est
revendiquée pour eux.

Réserves historiques maintenues : dépassement instrumenté AArch64 de 301 cadres
et reproductibilité de compilation entre environnements. Deux succès locaux
ne démontrent ni l'absence absolue d'intermittence ni la reproductibilité entre
toolchains. UTM, matériel physique et activation ordinaire ne sont pas qualifiés.

## Durées et verrou

Horodatages suivants en UTC (Europe/Paris = UTC+1 à cette date).

| Essai | Acquisition | Libération | Attente | Banc hôte |
|---|---|---|---:|---:|
| 01 | 15:41:38.612403 | 15:43:19.580919 | 0,000382 s | 100,923 s |
| 02 | 15:47:46.239296 | 15:48:22.083056 | 0,000380 s | 35,796 s |
| 03 | 15:49:08.265414 | 15:49:43.527827 | 0,000336 s | 35,219 s |

Total banc : 171,938 s ; attente : 0,001098 s, exclue du budget actif.
Le premier banc attend sa borne de disponibilité après le refus TLS ; cette
attente du banc est incluse dans l'exécution, pas assimilée à un succès.
Durées hôtes des deux parcours invités réussis : 34,994 et 34,435 s.
Constructions : 22,103 et 22,051 s, soit 44,154 s. Assemblage/contrôle ISO effectué
avant chaque candidat ; sa durée n'a pas été isolée à la seconde.

Le journal de travail commence à 15:38:53 UTC ; le travail antérieur n'a pas de
chronométrage continu et reçoit une provision conservatrice de 20 minutes dans
le budget. Développement/diagnostic et préparation se chevauchent avec certains
outils ; aucune somme prétendument exacte de ces catégories n'est inventée.
La borne conservatrice de travail actif et la durée d'archivage seront consignées
dans `bilan-temps.json`. Le budget de 90 minutes n'est pas approché.

Les trois acquisitions sont immédiates, sans campagne incompatible observée.
Chaque détenteur a libéré son propre verrou après nettoyage. Aucun verrou d'un
autre chantier n'a été supprimé ou forcé.

## Preuves et suite

Archive durable :
`/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/archives-novaauth-administration-0240/phase-r2-bc6ec3e/stabilisation-2cdfbd5`.
Les binaires exacts et données privées restent dans les répertoires
`artefacts-prives-administration-0240-r2/stabilisation-2cdfbd5` et
`artefacts-prives-administration-0240-r2/stabilisation-2cdfbd5-02`.
Les échecs précédents sont conservés. Les clés et les ELF qui les embarquent ne
sont pas publiés ; la synthèse versionnée ne contient aucune référence de session ni de ticket.

La suite recommandée, après décision MOA/MOE, est d'étendre ce témoin inchangé
aux trois autres profils, puis de reprendre les opérations et scénarios R2
encore incomplets. Aucun de ces essais n'est lancé ici. Les utilitaires restent
en pause. Aucune fusion, aucun tag, aucune activation ordinaire.

Vérification finale des archives : 143 fichiers de preuve relus, manifeste SHA-256 `a19eee539a7f38925efc1683c8e370f6b341798ed91328a61fddae23564593bf`.
Archive privée `stabilisation-2cdfbd5` : 20 fichiers relus ; manifeste `4be34ff169921273313c635532471016d8d67fa42a65e4b35f647bb49c01fb68`.
Archive privée `stabilisation-2cdfbd5-02` : 33 fichiers relus ; manifeste `b3d3a2e271577d027dac4e8cf96d9a58d806666e0e15cca7bfac189f909f7454`.

Borne conservatrice active à la fin des vérifications : 2215.6 s, dont une provision de 1 200 s pour la phase initiale non chronométrée. Archivage/rapport depuis son relevé dédié : 147.5 s. La publication suit ce relevé.

[Synthèse vérifiée](validation/novaauth-r2-stabilisation-20261122.json) · [Manifeste des preuves](validation/novaauth-r2-stabilisation-20261122.sha256).
