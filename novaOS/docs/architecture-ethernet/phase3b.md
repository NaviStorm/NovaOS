# Performances du port Ethernet v1 — Phase 3B

## Décision

Retenir la boîte de 4 Kio avec transferts et complétions groupés pour le profil expérimental. Les trois critères fixés avant optimisation sont atteints sur les deux architectures. Dans la cellule décisive, le débit utile a plus que doublé, la latence p99 diminue et le nombre d’appels IPC par trame baisse d’environ 66 %. Le repli DATA et les clients v1 qui utilisent la boîte trame par trame restent opérationnels.

| ISA | TX utile en écho, Mbit/s | Gain | p99 du lot, ms | Appels IPC par trame TX |
|---|---|---|---|---|
| AArch64 | 10,66 → 26,10 | 2,45× | 9,79 → 4,30 | 43,69 → 14,94 |
| x86_64 | 15,24 → 36,21 | 2,38× | 7,05 → 3,14 | 43,70 → 14,98 |

Les valeurs vont de la référence instrumentée au candidat. Le dénominateur IPC est le nombre de trames traitées dans un sens. En écho et en duplex, chaque trame TX est accompagnée d’une trame RX. Les compteurs couvrent les deux liaisons et la racine du courtier.

### Démarrages de la cellule décisive

| ISA | Variante | Débits TX successifs, Mbit/s | p99 successifs, ms |
|---|---|---|---|
| AArch64 | Référence | 10,85 / 10,48 / 10,66 | 9,53 / 10,03 / 9,79 |
| AArch64 | Candidat | 25,68 / 26,10 / 27,23 | 4,30 / 4,37 / 3,82 |
| x86_64 | Référence | 15,59 / 15,24 / 13,23 | 6,68 / 7,05 / 14,25 |
| x86_64 | Candidat | 35,04 / 36,21 / 36,92 | 3,35 / 3,14 / 2,99 |

Une contre-épreuve inverse l’ordre de passage à la deuxième répétition. Les images sont identiques à celles de la matrice principale. La dernière colonne est un démarrage ordinaire de la révision finale du code, avec les sondes chronométrées de veille et les injecteurs de panne désactivés.

| ISA | Référence appariée, Mbit/s / p99 ms | Candidat apparié, Mbit/s / p99 ms | Confirmation du code livré, Mbit/s / p99 ms |
|---|---|---|---|
| AArch64 | 10,56 / 10,03 | 26,53 / 4,51 | 27,54 / 3,87 |
| x86_64 | 15,52 / 6,79 | 37,50 / 2,88 | 37,91 / 2,89 |

## Ce que mesurent les séries

Le critère décisif porte sur huit trames de 1 514 octets en écho. Le débit retenu compte les octets TX de la trame Ethernet, en Mbit/s décimaux. Le p99 porte sur le lot entier. La mesure invitée commence à l’entrée du premier SEND de contrôle et se termine après vérification des reçus, restitution RX et acquittement du ticket. La préparation des descripteurs du client précède le premier SEND. Un appel de découverte IPC déjà autorisé délimite la fin du lot. Les appels de balisage sont écartés du décompte IPC. Le compteur architectural est converti avec la fréquence mesurée à chaque démarrage. [Critères figés](criteres-phase3b.md)

Pour chaque architecture et chaque variante, huit combinaisons direction × taille ont chacune trois démarrages indépendants. Chaque démarrage enchaîne 1 024 lots de une, quatre et huit trames. Chaque cellule réunit ainsi 3 072 lots. Les tableaux donnent la médiane des trois débits et la médiane des trois p99 calculés par rang supérieur. Les temps cumulés, les quatre quantiles et les observations de chaque démarrage sont conservés.

- En TX seul, le client attend la complétion et acquitte chaque ticket. Le pair injecte seize trames après la dernière émission pour permettre le drainage RX. Elles sont exclues de la mesure invitée.
- En RX seul, le pair émet le lot annoncé par le banc. En duplex, il démarre la réception indépendante sur le même signal, pendant la soumission TX. Un lecteur du journal série attend jusqu’à 0,5 ms entre deux lectures. Cette cadence intervient dans les latences RX et duplex. Le débit duplex additionne les octets des deux sens.
- En écho, chaque trame TX vérifiée déclenche la réponse correspondante du pair. Cette série mesure l’aller-retour de l’application jusqu’à la restitution. Le PCAP fournit en parallèle le débit effectivement observé au pair. La fenêtre de comparaison au pair utilise ses horodatages en microsecondes.
- Un seul QEMU occupe le verrou de qualification. Les compilations des séries retenues passent aussi sous ce verrou. Le PCAP, le contrôle du contenu au pair, les traces DMA et le traitement des tickets font partie du banc. Les premiers essais de mise au point et les ELF de diagnostic ont leurs propres résultats.

La campagne publiée de phase 3 indiquait 9,576 Mbit/s TX sur A72 et 13,448 Mbit/s sur Q35 pour sa série de 1 514 octets combinant plusieurs lots. La phase 3B rejoue le chemin initial avec la même instrumentation, la même distribution de lots et la même horloge que le candidat. Le ratio de décision utilise cette référence contemporaine. [Mesures de phase 3](phase3.md)

Les profils simples utilisent un vCPU TCG et 256 Mio. AArch64 emploie Cortex-A72, `virt-10.0,gic-version=3,its=off` et le modèle de laboratoire QEMU 10.0.12. Q35 emploie le CPU `max`, le modèle de laboratoire QEMU 11.1.3 et OVMF expérimental. Sur Q35, Limine scelle le BLAKE2b de l’ELF. Les variables Secure Boot sont enrôlées pour chaque démarrage. La consolidation vérifie la configuration calculée pour chaque ELF et conserve les empreintes individuelles. Le profil Q35 double avec NVMe a sa propre provenance.

## Débits et latences de la matrice

**Débit utile en Mbit/s.** Les colonnes TX et RX portent sur leur sens respectif, la colonne duplex sur la somme des deux sens et la colonne écho sur le TX du cycle aller-retour.

| ISA | Octets | Lot | TX seul | RX seul | Duplex | Écho |
|---|---|---|---|---|---|---|
| AArch64 | 60 | 1 | 0,35 → 0,54 | 0,38 → 0,67 | 0,44 → 0,70 | 0,22 → 0,33 |
| AArch64 | 60 | 4 | 0,64 → 1,67 | 0,78 → 2,15 | 0,77 → 2,01 | 0,37 → 0,98 |
| AArch64 | 60 | 8 | 0,76 → 2,46 | 0,93 → 3,43 | 0,87 → 3,06 | 0,42 → 1,51 |
| AArch64 | 1514 | 1 | 9,41 → 13,11 | 9,34 → 15,08 | 10,61 → 17,31 | 5,48 → 7,81 |
| AArch64 | 1514 | 4 | 16,38 → 34,39 | 17,77 → 34,37 | 18,58 → 39,94 | 9,24 → 19,28 |
| AArch64 | 1514 | 8 | 18,79 → 46,26 | 22,01 → 59,27 | 20,93 → 54,72 | 10,66 → 26,10 |
| x86_64 | 60 | 1 | 0,58 → 0,79 | 0,45 → 0,75 | 0,65 → 0,98 | 0,32 → 0,51 |
| x86_64 | 60 | 4 | 1,02 → 2,41 | 1,03 → 2,46 | 1,14 → 2,91 | 0,56 → 1,50 |
| x86_64 | 60 | 8 | 1,18 → 3,65 | 1,35 → 4,05 | 1,30 → 4,31 | 0,65 → 2,05 |
| x86_64 | 1514 | 1 | 13,78 → 19,16 | 10,70 → 18,42 | 15,15 → 23,47 | 7,86 → 11,99 |
| x86_64 | 1514 | 4 | 24,03 → 47,46 | 24,17 → 48,89 | 25,82 → 54,27 | 13,35 → 28,14 |
| x86_64 | 1514 | 8 | 27,02 → 64,64 | 33,76 → 61,20 | 30,32 → 70,02 | 15,24 → 36,21 |

**p99 du lot en millisecondes.** Pour l’écho, le temps inclut l’achèvement du cycle et l’acquittement du ticket.

| ISA | Octets | Lot | TX seul | RX seul | Duplex | Écho |
|---|---|---|---|---|---|---|
| AArch64 | 60 | 1 | 1,73 → 1,12 | 1,70 → 1,26 | 2,83 → 1,74 | 2,87 → 1,95 |
| AArch64 | 60 | 4 | 3,39 → 1,33 | 3,04 → 1,40 | 5,42 → 2,34 | 5,84 → 2,37 |
| AArch64 | 60 | 8 | 5,76 → 1,73 | 4,71 → 1,66 | 9,94 → 2,76 | 10,30 → 3,00 |
| AArch64 | 1514 | 1 | 1,70 → 1,26 | 1,80 → 1,37 | 2,95 → 1,77 | 2,88 → 2,11 |
| AArch64 | 1514 | 4 | 3,37 → 1,58 | 3,40 → 1,66 | 5,72 → 2,76 | 5,60 → 3,06 |
| AArch64 | 1514 | 8 | 5,66 → 2,37 | 5,23 → 2,06 | 10,13 → 3,87 | 9,79 → 4,30 |
| x86_64 | 60 | 1 | 0,95 → 0,72 | 1,49 → 0,82 | 1,75 → 1,18 | 1,82 → 1,18 |
| x86_64 | 60 | 4 | 2,16 → 0,97 | 2,25 → 1,08 | 3,72 → 1,69 | 3,98 → 1,65 |
| x86_64 | 60 | 8 | 3,74 → 1,31 | 3,16 → 1,32 | 6,44 → 2,21 | 6,68 → 2,34 |
| x86_64 | 1514 | 1 | 1,01 → 0,72 | 1,48 → 0,92 | 1,95 → 1,37 | 1,82 → 1,25 |
| x86_64 | 1514 | 4 | 2,31 → 1,23 | 2,34 → 1,54 | 4,40 → 2,14 | 4,07 → 2,07 |
| x86_64 | 1514 | 8 | 4,13 → 1,71 | 3,40 → 1,91 | 7,12 → 3,35 | 7,05 → 3,14 |

**Appels IPC invités par trame traitée dans un sens.** Le débit et le p99 sont calculés avec toutes les observations retenues, y compris les délais du pair.

| ISA | Octets | Lot | TX seul | RX seul | Duplex | Écho |
|---|---|---|---|---|---|---|
| AArch64 | 60 | 1 | 47,00 → 33,00 | 46,03 → 25,94 | 81,01 → 51,00 | 81,01 → 51,00 |
| AArch64 | 60 | 4 | 28,25 → 9,75 | 23,74 → 6,91 | 49,50 → 15,00 | 49,50 → 15,00 |
| AArch64 | 60 | 8 | 25,13 → 5,88 | 19,76 → 3,82 | 43,69 → 8,94 | 43,69 → 8,94 |
| AArch64 | 1514 | 1 | 47,00 → 33,00 | 43,53 → 27,32 | 81,04 → 51,00 | 81,00 → 51,00 |
| AArch64 | 1514 | 4 | 28,25 → 12,75 | 23,13 → 11,99 | 49,50 → 21,00 | 49,50 → 21,00 |
| AArch64 | 1514 | 8 | 25,13 → 8,88 | 19,54 → 6,85 | 43,69 → 14,94 | 43,69 → 14,94 |
| x86_64 | 60 | 1 | 47,00 → 33,00 | 54,12 → 32,15 | 81,00 → 52,43 | 81,00 → 51,00 |
| x86_64 | 60 | 4 | 28,25 → 9,75 | 26,06 → 9,61 | 49,51 → 15,42 | 49,53 → 15,04 |
| x86_64 | 60 | 8 | 25,13 → 5,88 | 20,92 → 5,39 | 43,69 → 8,97 | 43,69 → 9,56 |
| x86_64 | 1514 | 1 | 47,00 → 33,00 | 58,04 → 31,91 | 81,13 → 52,17 | 81,02 → 51,90 |
| x86_64 | 1514 | 4 | 28,31 → 12,75 | 27,01 → 12,21 | 49,55 → 21,04 | 49,60 → 21,04 |
| x86_64 | 1514 | 8 | 25,17 → 8,89 | 20,21 → 9,34 | 43,71 → 14,94 | 43,70 → 14,98 |

| Écarts observés | Cellules | Détail |
|---|---|---|
| Débit inférieur à la référence | 0 / 48 | Aucune cellule |
| p99 supérieur ou égal à la référence | 0 / 48 | Aucune cellule |
| IPC par trame supérieur ou égal à la référence | 0 / 48 | Aucune cellule |

Le débit double dans 29 cellules sur 48. Le plus petit gain est de 1,37× (x86_64, tx60, lot 1) et le plus grand de 3,68× (AArch64, rx60, lot 8).

## Modifications du port

Le bit Ethernet optionnel `BOITE_GROUPEE` (`1 << 32`) est négocié avant OPEN. Un client v1 qui ignore ce bit conserve la livraison trame par trame. DATA reste négociable à l’ouverture, y compris quand une boîte est disponible. Le choix vaut pour la session.

Les huit trames occupent au plus 12 112 octets de données privées à l’acteur. Le canal envoie successivement au plus 4 096 octets dans son unique boîte. Le lot de huit grandes trames utilise trois PUT et trois TAKE par liaison et par sens. Chaque segment intermédiaire attend l’acquittement du TAKE précédent. Le premier message de contrôle et l’échange suivant délimitent la propriété du lot. Le service attend les données complètes, contrôle les descripteurs et les filtres MAC/EtherType, puis admet les tickets. Une trame RX isolée et une préparation TX isolée empruntent directement le snapshot privé. Les chemins unitaires et DATA acceptent une source immuable, ce qui supprime leur copie de transit.

Le service prépare les coupons en groupe. Le pilote garde l’association des coupons aux descripteurs matériels et renvoie en un message le masque des complétions effectivement lues. Il groupe aussi les longueurs et les octets RX disponibles. Une consultation de ticket actualise les complétions avant de répondre quand aucun préfixe de publication ne reste en attente. Les tests de publication partielle et de réponse perdue exercent la conservation du suffixe et des reçus. Les anneaux restent à seize entrées. Les publications matérielles de une à huit trames, les contrôles du courtier et les permissions DMA gardent leur chemin audité.

Les campagnes de débit interrogent les reçus et la disponibilité RX. Elles n’arment pas de notification applicative pendant la mesure. Les essais `veille` et `filtre` exercent l’attente bloquante et son réveil. Le réarmement matériel intervient après la restitution des seize entrées RX.

## Profil des coûts

Les sondes détaillées ont leur propre démarrage pour chaque ISA, taille et variante. Chaque cellule comporte 1 024 lots. Les chiffres ci-dessous concernent huit trames de 1 514 octets en écho.

| Opérations ou octets par lot | AArch64 | x86_64 |
|---|---|---|
| Appels client ↔ service | 119,9 → 50,0 | 120,0 → 50,1 |
| Appels service ↔ pilote | 204,0 → 44,0 | 204,0 → 44,1 |
| Appels sur la racine matérielle | 25,5 → 25,5 | 25,5 → 25,5 |
| PUT et TAKE réussis | 64,0 → 24,0 | 64,0 → 24,0 |
| Octets copiés par PUT et TAKE | 96896 → 96896 | 96896 → 96896 |
| Changements de contexte | 405,8 → 132,2 | 405,5 → 132,0 |
| Entrées en attente bloquante | 186,6 → 44,2 | 186,4 → 44,1 |
| Réveils | 186,7 → 44,2 | 186,4 → 44,1 |
| Consultations de reçu | 2,0 → 1,0 | 2,0 → 1,0 |
| Notifications IPC | 0,0 → 0,0 | 0,0 → 0,0 |
| Réarmements matériels RX | 0,5 → 0,5 | 0,5 → 0,5 |
| Publications matérielles TX | 1,0 → 1,0 | 1,0 → 1,0 |

**Microsecondes cumulées par lot.** Les durées murales des acteurs se recouvrent. Les sondes de liaison suivent les appels entièrement compris dans le lot. Les sondes de reçu vont de la requête de contrôle à l’émission de la réponse correspondante. Le chronométrage du courtier couvre ses validations et ses copies.

| Périmètre | AArch64, µs | x86_64, µs |
|---|---|---|
| Appels client/service | 11355 → 4257 | 7750 → 3249 |
| Appels service/pilote | 11190 → 3187 | 7624 → 2429 |
| Appels IPC à la racine du courtier | 144 → 247 | 114 → 191 |
| Exécution des opérations du courtier | 214 → 211 | 236 → 222 |
| Attente bloquée du client | 8548 → 3121 | 5711 → 2283 |
| Attente bloquée du service | 6948 → 2759 | 4378 → 1839 |
| Attente bloquée du pilote | 3858 → 737 | 2460 → 646 |
| Copie dans la boîte, PUT | 100 → 27 | 109 → 96 |
| Copie depuis la boîte et effacement, TAKE | 124 → 34 | 178 → 111 |
| Admission TX jusqu’à son reçu | 1016 → 364 | 735 → 304 |
| Consultations QUERY jusqu’aux réponses | 4232 → 1735 | 2797 → 1213 |
| Demandes RX jusqu’aux réponses de contrôle | 2681 → 777 | 1843 → 643 |
| Restitution RELEASE | 53 → 55 | 39 → 39 |
| Acquittement ACK | 58 → 51 | 40 → 38 |

Dans le courtier, les compteurs d’opérations et la durée de médiation sont relevés sur le même lot. Le nombre couvre les tentatives, y compris une lecture sans trame disponible.

| Opération matérielle | AArch64, appels / µs par lot avant → après | x86_64, appels / µs par lot avant → après |
|---|---|---|
| Préparation TX | 8,00 / 92,5 → 8,00 / 91,4 | 8,00 / 83,8 → 8,00 / 78,6 |
| Collecte TX | 8,00 / 34,7 → 8,00 / 39,9 | 8,00 / 29,0 → 8,01 / 27,3 |
| Lecture RX | 8,00 / 68,4 → 8,00 / 64,0 | 8,00 / 109,2 → 8,01 / 104,0 |
| Réarmement RX | 0,50 / 9,4 → 0,50 / 8,5 | 0,50 / 6,5 → 0,50 / 5,8 |
| Publication TX | 1,00 / 8,8 → 1,00 / 6,8 | 1,00 / 7,5 → 1,00 / 6,2 |

### Notifications en attente bloquante

Ces mesures proviennent d’ELF de diagnostic distincts, avec trois démarrages par variante, architecture et scénario. Le pair envoie seize trames après le blocage du client. Dans le scénario filtré, seize trames étrangères précèdent les seize valides. Le service émet une notification RX pour chaque rafale de seize trames valides. Chaque cellule ci-dessous donne la médiane entre démarrages du temps moyen d’une opération, en microsecondes.

| ISA | Trafic | ARM, µs | NOTIFY, µs | NOTIFY → reprise, µs | Réarmement RX, µs |
|---|---|---|---|---|---|
| AArch64 | 16 valides | 104,0 → 102,0 | 47,0 → 42,0 | 102,0 → 132,0 | — → — |
| AArch64 | 16 filtrées + 16 valides | 114,0 → 100,5 | 47,0 → 42,0 | 111,0 → 131,0 | 144,0 → 178,0 |
| x86_64 | 16 valides | 88,2 → 80,3 | 53,1 → 47,2 | 87,2 → 121,4 | — → — |
| x86_64 | 16 filtrées + 16 valides | 79,2 → 74,0 | 52,2 → 45,0 | 90,3 → 102,0 | 189,5 → 199,2 |

Le client envoie deux contrôles ARM successifs. Le premier lit la révision, le second arme l’attente. La durée de reprise inclut l’ordonnancement. Le compteur de réarmement couvre les opérations de remise à disposition de l’anneau RX. Le tiret indique que les seize trames valides ont été drainées puis la session fermée sans seconde publication RX. Ces quelques événements ponctuels servent à attribuer un coût. Les p99 du port proviennent des longues séries de trafic.

Le temps médian entre l’entrée de NOTIFY et la reprise du client augmente sur les deux ISA. Sur une rafale valide, il passe de 102 à 132 µs sur AArch64 et de 87 à 121 µs sur x86_64. L’appel NOTIFY lui-même est plus court. Les intervalles peuvent se recouvrir lorsque l’ordonnanceur reprend le client avant le retour du service. Les trois observations par variante ne suffisent pas à estimer un p99 spécifique au réveil.

### Coût fixe et coût par trame

Pour une mesure C par lot de taille B, la pente vaut (C₈ − C₁) / 7. La partie fixe vaut C₁ moins cette pente. Le lot de quatre fournit un contrôle indépendant de la linéarité. Le modèle décrit l’écho de ce banc. Le franchissement des 4 Kio et la disponibilité des complétions interviennent dans les écarts.

| ISA | Octets | IPC par lot avant | IPC par lot après | Temps avant, ms | Temps après, ms | Écart après à B4, ms |
|---|---|---|---|---|---|---|
| AArch64 | 60 | 42,6 + 38,4 × B | 48,1 + 2,9 × B | 1,356 + 0,980 × B | 1,305 + 0,160 × B | -0,039 |
| AArch64 | 1514 | 42,8 + 38,3 × B | 41,2 + 9,8 × B | 1,319 + 1,006 × B | 1,110 + 0,301 × B | 0,065 |
| x86_64 | 60 | 42,6 + 38,4 × B | 47,6 + 3,4 × B | 0,861 + 0,626 × B | 0,840 + 0,120 × B | -0,052 |
| x86_64 | 1514 | 42,6 + 38,4 × B | 42,2 + 9,7 × B | 0,909 + 0,678 × B | 0,767 + 0,233 × B | 0,001 |

### Copies et propriété

En écho unitaire, une trame TX et sa réponse RX parcourent chacune deux liaisons. Les quatre PUT et quatre TAKE déplacent huit fois la longueur Ethernet. Chaque TAKE efface ensuite la zone consommée. Le compteur des copies de boîte mesure les appels réussis et leurs octets. Les ELF de diagnostic chronomètrent aussi l’effacement dans le TAKE.

Le journal TX et le slot RX prennent chacun un snapshot. L’adaptateur encode l’en-tête TX privé. Le courtier copie entre la page du pilote et son tampon, puis entre ce tampon et la mémoire DMA, dans chaque sens. Pour un aller-retour sans erreur, ces quatre copies du courtier portent chacune sur la longueur Ethernet augmentée de douze octets. Le code de l’anneau valide les bornes, le descripteur et la direction à la complétion.

En écho B8 de 1 514 octets, le trafic de boîte représente 96 896 octets pour les huit allers-retours, avant comme après regroupement. Cela compte les copies IPC des deux sens sur les deux liaisons. Le regroupement réduit leur nombre et le nombre de réveils, alors que le volume correspondant est conservé.

Le chemin unitaire initial faisait deux copies du snapshot vers un tampon de transit du service. Le chemin groupé les supprime et rassemble la réception dans le pilote avant remise des longueurs. Ces mouvements des acteurs sont identifiés dans le code. Les mesures directes d’octets et de temps portent sur les boîtes et les appels au courtier. La remise à zéro du journal à l’ACK et celle du slot au RELEASE participent au coût des reçus et de la restitution. [Canal et transferts](../../user/ethernet/canal.rs) · [Service Ethernet](../../user/ethernet/service.rs) · [Adaptateur](../../user/ethernet/adaptateur.rs) · [Propriété des snapshots](../../tests/ethernet_v1/lib.rs)

## Comparaison directe au pilote historique

Le courtier historique publie seize descripteurs à la fois. Le port v1 admet huit crédits et présente deux lots publics de huit pour seize trames au pair. Les deux chemins utilisent le même modèle, le même firmware de base et le même pair validant les octets. Les tailles de lot publiques de une et quatre n’ont pas d’équivalent historique direct.

Pour les grandes trames, chaque série utilise exactement les 8 176 trames comprises entre les passages de 5 136 et 13 312 trames au pair. Les empreintes des octets TX et RX coïncident entre les trois chemins. Le p99 du pair porte sur l’intervalle entre deux groupes successifs de seize TX. Chaque ELF historique termine ses 500 000 TX et RX.

| ISA | Chemin | TX mesuré au pair, Mbit/s | p99 intergroupes de 16 TX, ms |
|---|---|---|---|
| AArch64 | Port initial | 10,62 | 19,39 |
| AArch64 | Port groupé | 25,88 | 8,51 |
| AArch64 | Pilote historique | 37,38 | 5,57 |
| x86_64 | Port initial | 15,11 | 14,05 |
| x86_64 | Port groupé | 35,70 | 6,11 |
| x86_64 | Pilote historique | 48,46 | 5,31 |

Sur cette fenêtre de grandes trames, le port groupé atteint 69 % du débit du pilote historique sur AArch64 et 74 % sur x86_64. Le coût résiduel inclut les deux liaisons IPC et la restitution explicite des tickets. Le courtier reçoit encore environ 25,5 appels matériels par lot de huit en écho sur les deux architectures.

Un second parcours utilise cent mille trames alternant 60 et 1 514 octets avec la même séquence sur les deux pilotes. La fenêtre centrale couvre 99 488 trames, entre les passages 256 et 99 744. Le pair chronomètre les groupes avec son horloge monotone.

| ISA | TX au pair, historique → v1, Mbit/s | p99 intergroupes, historique → v1, ms |
|---|---|---|
| AArch64 | 54,70 → 17,32 | 2,30 → 6,77 |
| x86_64 | 38,39 → 21,94 | 6,79 → 5,97 |

## Ressources

Les valeurs ci-dessous sont les médianes des trois démarrages écho de 1 514 octets. Chaque démarrage traite 13 312 trames par sens. Le temps CPU hôte couvre la VM du démarrage au retrait, avec son firmware et le contrôle des périphériques. Les échantillons de l’ordonnanceur donnent la part noyau dans l’invité.

| ISA | Pages à l’ouverture | Pages au premier TX | Cadres DMA observés | Changements de contexte | Réveils | CPU hôte, s | Échantillons noyau invité, % |
|---|---|---|---|---|---|---|---|
| AArch64 | 300 → 301 | 300 → 301 | 38 → 38 | 760507 → 289906 | 354138 → 110472 | 18,0 → 9,0 | 91,6 → 84,7 |
| x86_64 | 299 → 301 | 299 → 301 | 30 → 30 | 763607 → 291257 | 356077 → 111466 | 15,2 → 8,8 | 88,2 → 79,7 |

Un cadre fait 4 096 octets. Le coût à l’ouverture et à la première publication est la différence de cadres libres par rapport à la situation avant création des acteurs. La colonne DMA donne le total observé au point de publication sur la machine. Le banc réserve trois piles utilisateur de 256 Kio, soit 192 pages. Un `Trames` de travail sur pile contient 12 112 octets de données, neuf bornes et un compteur, soit 12 192 octets sur les deux cibles. Cette valeur est la taille d’un objet local et ne mesure pas la profondeur maximale de pile.

Les racines IPC du client, du service et du pilote comptent respectivement 0, 4 et 1 racines de pilote. Les délégations du client passent par ses handles IPC. Le profil ordinaire utilise deux boîtes, soit 8 Kio de données, cinq endpoints et neuf pages IPC au relevé de session. Le scénario de saturation compte huit boîtes réservées, soit 32 Kio, treize endpoints et vingt-trois pages IPC, réservations du banc incluses. Le repli sans boîte et le choix explicite de DATA en présence de boîtes sont exercés séparément.

La table historique des 16 384 temps de lot représente 128 Kio statiques dans le noyau de laboratoire. Les 30 compteurs de trois tailles de lot représentent 720 octets. Les ELF de diagnostic ajoutent notamment 624 octets de couples compte/ticks pour les treize classes par taille et 96 octets pour les quatre points de veille. Les pages de données des boîtes sont distinctes des cadres DMA du NIC.

## Non-régression et pannes

Les treize scénarios ci-dessous sont exécutés sur chacune des deux architectures. Les neuf tests ciblés du contrat passent en Darwin ARM64 natif ainsi qu’en x86_64 sous Rosetta. Ils couvrent aussi les incarnations, les générations, les droits à l’ouverture, la saturation du journal et le réveil sur révocation. Les neuf contrôles du registre passent. La table ABI garde ses 48 sélecteurs. Quatre liens du registre généré suivent le déplacement des lignes du répartiteur.

| Scénarios QEMU | Observation exigée |
|---|---|
| `boite`, `ancien`, `data`, `choix-data`, `plein` | Lots de une à huit trames, reçus et octets au pair. Client antérieur sur boîte unitaire, choix explicite de DATA et repli quand les huit boîtes sont déjà réservées. Le courtier refuse une neuvième préparation dans le cas nominal. |
| `veille`, `filtre` | Client réellement bloqué puis réveillé, seize trames RX valides. Le second scénario injecte d’abord seize trames à MAC source ou EtherType étranger. Le pilote les écarte et poursuit après le réarmement. |
| `deces`, `incertain`, `revocation` | Publication partielle de quatre trames observée au pair. Réconciliation des tickets et conservation des ressources incertaines. |
| `court` | Fermeture après une trame. L’anneau RX partiellement utilisé entraîne un retrait en quarantaine. |
| `segment-interrompu` | Le client quitte après la confirmation du premier TAKE de 4 Kio. Le pair observe zéro TX et le pilote a seulement ouvert la porte matérielle. |
| `recu-collecte` | Le pilote quitte après la lecture des complétions et avant la remise du masque. Huit TX uniques sont observés au pair. Deux lectures confirment les huit statuts incertains et l’acquittement est refusé. |

La série de 100 000 trames par sens franchit le bouclage de l’indice 16 bits. Les filtres et permissions DMA directionnelles restent ceux du courtier audité. Le client et le service n’obtiennent aucun handle matériel. Le profil Q35 double a effectué 32 opérations NVMe dans chacun des deux scénarios. Vingt-cinq opérations ont abouti après le retrait nominal du NIC et trente et une après le décès du pilote. La relecture indépendante de P9 confirme le contenu de `/sortie` dans les deux cas. Les sept acteurs respectent le plafond de quatre racines.

## Périmètre et provenance

La phase 3B mesure l’émulation TCG et le pair de laboratoire. Le service conserve son contrat Ethernet indépendant du backend. Les résultats dépendent du QEMU corrigé, du firmware de laboratoire et des coûts de supervision et de journalisation du pair. Le nombre de mesures par démarrage caractérise le p99 de ces cellules. Les trois redémarrages renseignent la dispersion entre exécutions sur cet hôte.

Le parcours NovaFS historique qui retourne `BUSY (24)`, le débit du service de fichiers sous trafic prolongé, le réemploi des pages DMA en quarantaine et la reproductibilité du fichier ELF complet restent ouverts. Le contrôle vérifie le SHA-256 de l’image réellement exécutée. Les scénarios applicatifs simultanés, les performances physiques, la pile IP et l’installation dynamique ne font pas partie de cette qualification. [Réserves et provenance de la phase 3](phase3.md)

| Révision | Rôle |
|---|---|
| `dc498394108210f0479189a30c3e5d91de601c78` | Point de départ de phase 3, rapports et captures historiques préservés |
| `a18d23d586187bce3b94bf8a0f351d045ebe8147` | Critères fixés avant optimisation. Le libellé du commit indique v0.24.0 par erreur. Le manifeste du composant porte v0.20.0. |
| `cdf568194dbbc357b8c69d92153a1b7f04505e53` | Référence instrumentée, chemin de données de phase 3 |
| `eaa14b9b578b74993a253a3000d0f8ea8559a5f6` | Boîte et complétions groupées, mise au point de la synchronisation du pair |
| `ff864f5bec14315e7374375a92ca47912a0abdc5` | Candidat de performance, suppression des copies de transit et sondes de diagnostic séparées |
| `52d64291a7729261ac5dba24ab4a3b66fe337ca0` | Deux images de panne supplémentaires et confirmation du chemin ordinaire sur chaque ISA |
| `1a7de0e80b91e8b94525ef1e337d8daed97e0406` | Chronométrage de la veille réservé au diagnostic, contrôle du chemin ordinaire livré sur les deux ISA |
| `56db7d5c0d04490cd284b91567cbbd4736534266` | Audit historique consulté en lecture seule. Les témoins de 100 000 trames et de 500 000 grandes trames proviennent des ELF archivés de leurs campagnes respectives. |

Le premier essai exploratoire RX AArch64 a démarré la lecture du journal avant sa création. Le pair attend désormais sa création. L’archive conserve cet échec de lancement avec sa provenance. Toutes les cellules principales utilisent le lanceur corrigé. Les ordres de passage, périodes d’attente du verrou, arguments QEMU, constructions, empreintes des firmwares et observations individuelles sont consignés dans les preuves.

Le bilan retient 190 démarrages QEMU. La matrice avant/après en représente 96, le diagnostic détaillé de trafic 8, la contre-épreuve à ordre alterné 12, les comparaisons au pair 18, les scénarios de non-régression 26, le stockage Q35 double 2, les sondes de veille 24 et les confirmations du chemin ordinaire 4.

## Reproduction et preuves

Pour chaque mode, l’ELF AArch64 et l’ELF x86_64 proviennent du manifeste `construction.json` de la variante. Pour le diagnostic de la référence, la construction superpose les six fichiers noyau des sondes au commit de référence instrumenté. Le diagnostic de veille y ajoute le septième fichier `profil_notifications.rs`. Le manifeste énumère leurs blobs Git. Les campagnes principales utilisent les images ordinaires. Les exécutables historiques proviennent des archives identifiées par SHA-256.

Depuis la racine du worktree Ethernet, avec le Python de laboratoire et `NOVAOS_TMPDIR` sous le répertoire partagé des worktrees, la commande suivante rejoue une cellule à partir des images conservées. Le lanceur acquiert le verrou, vérifie l’empreinte de l’ELF et préserve les répertoires d’essais existants.

```sh
python -B novaOS/tests/ethernet_v1/performance.py rejouer \
  ../archives-ethernet-phase3b/images-ff864f5 echo1514 \
  --sortie ../archives-ethernet-phase3b/rejeu-local --repetitions 3
```

`performance.py compiler SORTIE MODES --revision SHA` reconstruit depuis une archive Git isolée. L’option `--detail` ajoute les sondes chronométrées. Le registre ABI se vérifie avec `python -B novaOS/scripts/generate_abi.py --check` et `python -B novaOS/tests/abi_registre.py`. Les journaux, commandes, observations de chaque démarrage et empreintes des traces complètes accompagnent le rapport. [Bilan machine](../validation/ethernet-v1-phase3b/bilan.json) · [Archive de preuves](../validation/ethernet-v1-phase3b/preuves.tar.gz) · [Empreintes](../validation/ethernet-v1-phase3b/SHA256SUMS) · [Vérificateur](../../tests/ethernet_v1/performance_bilan.py)

Les archives brutes durables se trouvent dans `../archives-ethernet-phase3b/` à côté du worktree. Ce répertoire conserve les PCAP complets, traces, journaux et ELF pour chaque démarrage. L’archive publiée contient l’inventaire des SHA-256, les journaux, les paramètres, les observations et une sélection de trames vérifiables. Pour refaire la consolidation dans un répertoire neuf, exécuter les commandes suivantes depuis la racine du worktree avec Python 3.12.

```sh
mkdir -p "$NOVAOS_TMPDIR/verification-ethernet-3b"
python -B novaOS/tests/ethernet_v1/performance_bilan.py \
  ../archives-ethernet-phase3b "$NOVAOS_TMPDIR/verification-ethernet-3b/bilan.json"
cmp "$NOVAOS_TMPDIR/verification-ethernet-3b/bilan.json" \
  novaOS/docs/validation/ethernet-v1-phase3b/bilan.json
(cd novaOS/docs/validation/ethernet-v1-phase3b && shasum -a 256 -c SHA256SUMS)
```
