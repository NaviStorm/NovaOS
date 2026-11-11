# NovaOS v0.9.0 — Service utilisateur de stockage

Au démarrage, un client consulte le service NovaFS en Ring 3 sur x86_64 ou en EL0 sur AArch64. La réponse identifie le service et annonce **zéro volume attaché**. Le client ferme sa session et termine. Le serveur reste en attente dans son propre espace d’adressage. Le moniteur série affiche le PID de chaque interlocuteur et la confirmation de la session. [Client](../user/storage/client.rs) · [Serveur](../user/storage/service.rs)

## Audit du commit de départ

La base est `3294ebf48849de02b97e57b6990205ed66b891dd` sur `main`. Le manifeste du noyau y indiquait 0.8.0, celui de NovaFS 0.9.12. Les licences sont respectivement MIT et MPL-2.0.

| Mécanisme examiné | Constat dans le code et conséquence |
|---|---|
| ABI et IPC | Le dispatcher traite six sélecteurs natifs et dix sélecteurs IPC. Le domaine IPC vérifie le propriétaire, les droits et l’incarnation de la capacité. Les copies valident toutes les pages, y compris après une attente. Le service utilise ces portes existantes. |
| Isolation et chargement | Le lanceur installe une image plate à `0x400000` (RX), une page de configuration à `0x401000` (RW/NX) et une pile utilisateur de 4 Kio entre deux gardes. Chaque processus a ses tables et sa pile superviseur. Le build lie le client et le serveur séparément pour chaque cible, puis incorpore leurs octets à l’ELF du noyau. Seul le processus utilisateur exécute leur code. |
| Publication | Le lanceur IPC de la v0.8.0 savait créer plusieurs processus atomiquement et distribuer une capacité de bootstrap. La v0.9.0 lui ajoute un canal de retour et l’admission successive du client désigné. Les anciens numéros, descripteurs et conventions de registre restent identiques. |
| Stockage invité | Le firmware lit le CD pour charger Limine. Après transfert au noyau, il manque un pilote et un service bloc accessibles aux processus. Le service annonce l’inventaire réellement disponible. |
| Moteur hôte | Le trait `Device` manipule déjà des blocs de 4 Kio et un flush explicite. Son contrat de verrou expose `std::sync::Arc<Mutex<()>>`. L’erreur d’E/S encapsule `std::io::Error`. Le laboratoire utilise `FileExt`, `File::sync_all` et des fichiers Darwin. La source d’entropie de l’adaptateur hôte appelle `getrandom`. |

Sources de l’audit. [Processus et collecte](../kernel/src/sched/process.rs) · [Pages utilisateur](../kernel/src/memory/user.rs) · [Appels natifs](../abi/native.rs) · [Transport et validation des tampons](../kernel/src/sched/process/ipc/traffic.rs) · [Interface Device](../../novaFS/src/lib.rs) · [Adaptateur fichier](../../novaFS/lab/device.rs) · [Entropie](../../novaFS/src/crypto.rs)

## Autorité et cycle de vie

1. Le chargeur UEFI démarre le noyau, qui initialise MMU, tas et ordonnanceur. En démarrage graphique normal, après la démonstration historique, un thread supervise le service indépendamment du thread d’interface. Sur Q35 sans framebuffer, la session s’exécute sur diagnostic série, puis le superviseur ferme le service avant l’arrêt historique du CPU.
2. Le lanceur crée les deux processus et deux endpoints unicast sous le même masque IRQ sur CPU0. Le serveur possède les deux racines. Le client reçoit **SEND** sur les demandes et **RECEIVE** sur les réponses. Il obtient la première clé par `IPC_BOOTSTRAP` et la seconde dans le premier mot de sa DATA privée. Le second mot configure le nombre d’échanges du client intégré. Toute erreur de provisionnement retire les capacités, les TCB et les mappings de la publication inachevée.
3. Seul le lanceur de confiance choisit le binaire client. Le marqueur `NVFS` identifie le protocole. L’autorité provient des capacités locales accordées aux processus. Le client ne peut ni lire les demandes, ni écrire des réponses, ni déléguer ou révoquer ces droits. L’adresse d’un descripteur appartient à l’appelant et chaque réception recopie les octets dans sa mémoire privée.
4. Une réponse synchrone à `FIN` clôt la session. Le serveur recommence la numérotation et retourne en réception. Le superviseur vérifie que le premier processus est collecté, que les deux files et les notifications sont vides et que le serveur attend une demande avant de lancer une nouvelle instance du même client. Les nouveaux handles appartiennent au nouveau PID. Le lanceur rejette une admission simultanée.
5. Le superviseur peut fermer les deux racines, ce qui réveille les appels bloqués. La mort du serveur détruit aussi ses endpoints. Un appel déjà en attente peut renvoyer `CLOSED`, une nouvelle recherche avec une clé invalidée renvoie `DENIED`. Une défaillance lance au plus une seconde tentative, avec de nouveaux PID et de nouvelles racines. Le superviseur dort sur l’événement de terminaison du processus résident. Après épuisement des tentatives, il signale l’indisponibilité sur le port série. La console et l’ordonnanceur restent autonomes.

Le démarrage ordinaire associe les deux images utilisateur intégrées. Le banc dispose en plus d’une image qui termine pendant une requête. Le prototype n’offre ni recherche publique par nom, ni lancement à la demande depuis une application, ni clients concurrents. Un exécutable tiers qui boucle en calcul demanderait une politique de surveillance et d’arrêt forcé supplémentaire. L’assemblage futur pourra fournir les images depuis une distribution système immuable et signée. La version actuelle les sélectionne au build. [Lanceur et supervision](../kernel/src/sched/process/storage.rs) · [Attribution et retrait des capacités](../kernel/src/sched/process/ipc/mod.rs)

## Protocole stockage v1

Le protocole est transporté par les descripteurs v1 de `IPC_SEND` et `IPC_RECEIVE`. Sur le fil, les champs multioctets sont little-endian. Une requête utilise le type DATA, un tag IPC `u64` non nul et strictement croissant dans la session, et exactement huit octets de charge utile. Un seul échange est en vol. Le client vérifie le type, le tag écho, la taille, la signature et tous les champs de la réponse.

| Octets | Demande | Réponse |
|---|---|---|
| 0 | Version 1 | Version 1 |
| 1 | Opération 1 pour `INFO`, 2 pour `FIN` | Statut applicatif |
| 2–3 | Réservés, zéro | Réservés, zéro |
| 4–7 | Cible `u32`. Zéro désigne le service | `u32` little-endian. Nombre de volumes pour `INFO`, zéro pour `FIN` et les erreurs |
| 8–11 | Fin du message à l’octet 8 | Identifiant ASCII `NVFS` |

Les statuts sont 0 pour succès, 1 pour trame invalide, 2 pour opération inconnue et 3 pour cible de volume indisponible. La vérification porte dans l’ordre sur tag et taille, version et réservés, cible, puis opération. Toute trame de huit octets avec un nouveau tag consomme cet identifiant même si son contenu est rejeté. Une trame malformée ne termine pas la session. Une notification ne porte aucune demande. Le serveur refuse les offres de capacités au niveau IPC. La bibliothèque retourne séparément les erreurs de transport. Les demandes et réponses synchrones expirent après 250 ticks, soit une seconde à 250 Hz. En attente d’une première demande, le serveur renouvelle sa réception après 15 000 ticks. [Bibliothèque native du client et du serveur](../user/storage/runtime.rs)

## Portage du moteur et sécurité du stockage

Le prochain prérequis d’E/S serait un service bloc capable de rendre séparément « écriture acceptée » et « synchronisation confirmée ». Un adaptateur invité devra préserver les deux barrières transactionnelles et traiter les issues indéterminées. Le découplage du moteur requiert des erreurs portables, des collections et allocations utilisateur bornées, une exclusion de mutation indépendante de Darwin et une source d’entropie adaptée. Un transfert de page ou une fragmentation explicitement spécifiée sera nécessaire pour faire passer un bloc de 4 Kio par un transport à messages de 128 octets.

Les opérations de données, la gestion des clés, les snapshots, les clones, LZ4, les exports et le scrub restent ceux du prototype NovaFS 0.9.12 sur macOS. Cette version du service transporte uniquement des informations de session. Le binaire invité n’ouvre aucune image et ne reçoit aucune clé. La qualification du stockage durable et de son format reste celle du moteur hôte. [Prototype NovaFS](../../novaFS/README.md)

## Validation QEMU

Hôte macOS 27.2 ARM64, Rust stable 1.99.1, QEMU 11.1.3 en TCG, un vCPU. La qualification principale utilise 256 Mio, EDK II et Limine, sur Q35 et sur `virt` Cortex-A72. Le banc ABI utilise aussi un Cortex-A76 pour exercer PAN.

Le banc vérifie dans les deux VM les codes de sortie en mode utilisateur, les PID et les observations suivantes.

- Un processus sans bootstrap reçoit `DENIED`. Dans une session admise, le client vérifie le refus des quatre opérations interdites (réception, envoi dans le sens inverse, révocation, duplication).
- Le client vérifie tag nul et répété, longueur tronquée, version inconnue, champ réservé non nul sur `FIN`, cible absente et opération inconnue. Il exige ensuite une réponse `INFO` correcte.
- Le serveur de panne accepte la première demande et sort avec le code 70 avant toute réponse. Le client reçoit l’erreur de transport. La seconde tentative démarre un nouveau serveur, qui traite successivement deux PID clients. La session normale reste disponible après le premier `FIN`. Le superviseur l’arrête ensuite et vérifie la restitution.
- Une publication sans son dernier cadre physique échoue avec `NO_MEMORY`. Le banc compare les cadres libres et le Buddy avant et après l’opération, les slots de processus et les pages IPC. Le test de revalidation historique T10 continue de revalider la copie entre pages.
- Les cinq images de référence du framebuffer ont été recalculées à partir des captures de la v0.8.0 dont l’empreinte était connue. Seuls les pixels du chiffre de version ont changé. QEMU compare les nouvelles images intégrales et exerce composition AZERTY, retour arrière et défilement. Le premier essai x86_64 a révélé que le script de liaison supprimait la GOT utilisée pour `memcpy` et `memset`. Les adresses résolues statiquement résident désormais dans la même page RX que le code. [Script de liaison](../user/demo.ld) · [Oracles QEMU](../tests/demarrage.py)

## Résultats mesurés

Les deux suites `make test` ont chacune validé 25 démarrages QEMU. L'ABI native, l'isolation, les exceptions contrôlées, le tas, la préemption, le framebuffer et les scénarios IPC historiques ont été exercés. T09 et T17 ont été exécutés séparément par `ipc-races` et `ipc-perf`. La matrice RAM a validé ABI et IPC à 128 et 256 Mio sur les deux cibles. À 64 Mio, Q35 s’arrête dans l’allocateur de Limine et `virt` dans EDK II avant l’entrée du noyau.

| Campagnes complémentaires | x86_64 | AArch64 |
|---|---:|---:|
| Ordres de réveil × répétitions | 5 × 10 000 | 5 × 10 000 |
| Préemptions observées dans T09 | 503 | 447 |
| Séries chronométrées dans T17 | 60 | 60 |
| Endurance ordonnanceur, secondes hôte | 15,746 | 20,831 |
| Endurance IPC, secondes hôte | 15,085 | 15,044 |
| Cycles IPC complets en endurance | 1 874 | 1 500 |

Les campagnes de deux heures des versions antérieures restent des résultats historiques. Ici, les endurances ont été demandées pour 15 secondes, avec arrêt au premier bilan complet. Les builds release verrouillés de NovaFS 0.9.12 ont également abouti pour les deux cibles Darwin. Le diff de `novaFS/` est vide et ses tests métier n’ont pas été relancés.

### Mémoire physique invitée

| Phase (Kio) | x86_64 | AArch64 |
|---|---:|---:|
| Noyau au repos dans la VM du banc | 1 212 | 1 140 |
| Service en attente après collecte du client | 1 336 | 1 264 |
| Surcoût du service en attente dans le banc | 124 | 124 |
| Deux processus publiés, avant leur première instruction | 1 428 | 1 356 |
| Pic pendant panne, reprise et second client | 1 432 | 1 360 |
| Après fermeture et collecte | 1 212 | 1 140 |
| Démarrage normal, système et service résidents | 1 400 | 1 264 |
| Démarrage normal, RAM retenue avec vidéo et frais de chargement | 1 400 | 3 140 |

Le pic est mesuré à chaque allocation physique réussie, avec une fenêtre ouverte avant le client sans bootstrap et fermée après la seconde session. Sur chaque cible, 161 allocations de cadres ont été observées, avec au plus 55 cadres supplémentaires simultanément. Au repos après normalisation, le Buddy compte trois blocs occupés (9 280 octets sur x86_64 et 10 304 sur AArch64). Après la pénurie provoquée avec 53 cadres disponibles, les compteurs et les cadres libres retrouvent leurs valeurs de départ. Le surcoût de 124 Kio du service en attente comprend trois pages utilisateur, seize pages de pile superviseur, sept pages de tables et cinq pages IPC. Le service et le client utilisent uniquement leur pile et leurs pages statiques.

La mesure précédente à 256 Mio (`ipc_repos`) était de 1 192 Kio sur Q35 et de 1 120 Kio sur `virt`. L’écart à phase équivalente correspond à cinq pages d’image liée sur chacune des cibles. La vidéo de `virt` retient 469 pages de RAM système. Sur Q35, le framebuffer VGA est compté séparément. La phase de démarrage normal inclut le superviseur et, sur x86_64, le thread du clavier.

### Échanges et changements de contexte

| Campagne de 512 INFO et un FIN | x86_64 | AArch64 |
|---|---:|---:|
| Aller-retour p50 (µs) | 30,67 | 40,96 |
| Aller-retour p95 (µs) | 32,71 | 45,06 |
| Aller-retour p99 (µs) | 49,07 | 53,25 |
| Aller-retour max (µs) | 240,59 | 841,01 |
| Échanges complets par seconde | 31 927 | 23 834 |
| Commutations directes chronométrées | 1 026 | 1 025 |
| Commutation p50 (µs) | 3,07 | 4,10 |
| Commutation p95 (µs) | 4,09 | 5,12 |
| Commutation p99 (µs) | 4,09 | 5,12 |
| Commutation max (µs) | 4,99 | 9,01 |

Pour les mesures publiées, les deux architectures ont été chronométrées successivement, avec une seule VM du banc en exécution. Le chronomètre de l’aller-retour part à l’entrée du SEND du client et s’arrête à la livraison de sa réponse. Le débit utilise la fenêtre du premier envoi à la dernière livraison. La mesure de commutation couvre la sélection, le changement de racine et de pile et le retour dans une continuation déjà suspendue chez le pair. Les passages par idle et les nouvelles entrées sont exclus. Les quantiles utilisent la borne supérieure des classes du banc v0.8.0. Les maxima sont exacts au pas du compteur invité. Les valeurs dépendent de TCG et de la calibration du compteur, particulièrement sur x86_64.

| Ancien banc DATA128 sans acteur de fond, 50 000 tours | x86_64 | AArch64 |
|---|---:|---:|
| v0.8.0, p99 (µs) | 49,26 | 61,44 |
| v0.9.0, p99 (µs) | 53,02 | 65,54 |
| v0.8.0, tours/s | 24 413 | 19 928 |
| v0.9.0, tours/s | 23 711 | 19 606 |

La comparaison utilise les archives v0.8.0 et une nouvelle exécution de la même charge. Les résultats chiffrent l’écart entre les campagnes. Une attribution de cet écart à une modification du noyau demanderait plusieurs séries appariées à charge hôte contrôlée. [Mesures historiques](mesures-v0.8.0.md)

ELF x86_64 testé `b0e81628376e2517af9b36711a4d5a43e4434fa5ad546024004a3a38e5bb09d0`. ELF AArch64 testé `a3a7ee3139e5dfcc8f19522c1d5e4bfef53b6811ca428bb07d1a4814a562abf8`.

## Reproduction depuis la racine Git

```sh
for arch in x86_64 aarch64; do
    make -C novaOS ARCH="$arch" test
    make -C novaOS ARCH="$arch" storage-test
    make -C novaOS ARCH="$arch" ram-test ipc-races ipc-perf
    make -C novaOS ARCH="$arch" ipc-endurance DURATION=15
done
make -C novaOS ARCH=x86_64 run
make -C novaOS ARCH=aarch64 run
```

Le scénario dédié conserve les mesures dans `novaOS/build/serie-<architecture>-storage-test.log`. Les démarrages ordinaires conservent leur bilan résident dans `novaOS/build/serie-std.log` et `novaOS/build/serie-aarch64.log`. Les commandes `run` ouvrent la fenêtre QEMU et dirigent la série vers le terminal. Le clavier AZERTY est disponible sur Q35.
