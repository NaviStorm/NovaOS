# NovaOS v0.25.0 · Phase 6A · Diagnostic visible du noyau

**Le candidat de 4 005 560 320 octets atteint le verdict terminal sous QEMU avec 1, 2 et 4 vCPU.** Le moniteur a observé l’entrée ELF, l’exécution de la première tâche ordonnancée et l’arrêt de chaque processeur. Un second contrôle a relu les pixels affichés. A8 reste fermé pour le noyau. [Capture nominale à quatre CPU](validation/live-noyau-6a/verdict-4cpu.png) · [Candidature pour un essai unique sur le MSI](candidature-a8-noyau-6a.md)

## Image qualifiée

Base 5Z `76cc5888cfdaec2b33e950a6d4047610294f1ac5`. Deux constructions indépendantes ont produit le même fichier disque. L’extraction de la livraison compressée et la comparaison octet par octet avec la construction qualifiée ont également réussi. La seconde GPT occupe la dernière LBA, 7 823 359. NOVAOS et NOVALOGS sont deux FAT32 distinctes. Les 24 emplacements NOVALOGS sont vides. Le résultat reste à l’écran, en RAM et sur la série du banc. [Image compressée](validation/live-noyau-6a/novaos-live-x86_64.img.gz) · [Empreintes des fichiers distribués](validation/live-noyau-6a/SHA256SUMS)

| Objet | SHA-256 |
|---|---|
| Image brute, capacité exacte de la Kingston | `9945b4334170495d9a9836f1f8c9224c8293600b33445fc8f7c4d799ba26961a` |
| Premier EFI, chemin amovible `EFI/BOOT/BOOTX64.EFI` | `440c7a0126dafea180cbd57c5073d4277985b53a883dfaf49b5ec4c8401c8119` |
| Limine avec configuration liée | `6bd2125fc79fc5c6ae01545df8c78f340a78722e17dcb730eea89f4ac1391484` |
| Noyau `NOVAOS.ELF`, 4 806 880 octets | `f2733be903aa27016c6271b76b13f4c4b4ce9997c0e3fa8ac9c733c9fcb240e1` |
| Configuration Limine | `80a21c2aef6f5c5bae6d8179ba62d0cd0dee2c6572baa1870ba053797917c624` |
| Plan de chargement | `f11887a649301d06fc80855984dcc4563521e90d7e55622945907823be4050a5` |

Le premier EFI utilise la chaîne 5Z et vérifie les composants depuis sa partition USB. Limine charge le noyau x86_64 avec le scénario fixe `smp-test`. Avec Secure Boot désactivé, l’intégrité du premier EFI dépend du contrôle externe du support. Le profil vidéo est un choix de construction du noyau. Il fonctionne sans identifiant MSI ou Lenovo, entrée clavier, pilote GPU, réseau ou stockage. La version officielle, le manifeste interne et les 48 sélecteurs restent ceux de la base. [Chaîne UEFI et limites de confiance 5Z](diagnostic-uefi-5z.md)

## Jalons et vérification indépendante

| Jalon affiché | Observation extérieure à l’affichage |
|---|---|
| Entrée noyau | Retour réussi de `ExitBootServices`. Point d’arrêt sur la première instruction de `nova_start`, vérification de RIP et des 32 premiers octets contre l’ELF. Le panneau n’existe pas encore à cette instruction. |
| Mémoire initialisée | Arrêt au début de `sched::start`, après installation du tas. Lecture de CR3, des registres de protection et des tables de pages dans la mémoire physique de la VM. Le panneau porte les deux premières lignes. |
| Ordonnanceur actif | Arrêt à l’entrée de `main_thread` puis contrôle après l’exécution de cette première tâche. Le journal et les pixels passent de deux à trois jalons. |
| SMP vérifié | Rendez-vous du scénario SMP, progression des tâches préemptées, stationnement des secondaires et nombre effectivement admis. Le panneau distingue 01, 02 et 04 CPU. |
| Verdict terminal | État des vCPU après reprise des horloges du banc pendant 250 ms. Tous présentent `HLT=1` et `IF=0`. Deux captures successives de l’écran complet sont identiques. L’arrêt local conserve le verdict à l’écran. |

Le journal borné en RAM comprend un mot d’état et huit événements de 64 bits. La sortie série existante conserve ses traces. Les captures QMP couvrent le scanout complet de 1 280 × 800 pixels. Le vérificateur reconstruit le texte à partir de la police de référence et compare les 202 752 pixels d’un panneau de 768 × 264, espaces et fond compris. La première ligne « entrée » est aussi visible seule dans les refus précoces. [Observations QMP, série, tables CPU et pixels](validation/live-noyau-6a/observations.tar.gz)

## Résultats QEMU du 28 novembre 2026

Banc Q35 x86_64, QEMU 11.1.3 TCG, OVMF standard, Secure Boot désactivé. Le même fichier disque virtuel USB est présenté en lecture seule. Un disque SATA témoin et une carte e1000e sont présents pendant la mesure.

| RAM | vCPU actifs au rendez-vous | Jalons | E/S SATA entre la première instruction EFI et l’arrêt local |
|---:|---:|---|---:|
| 256 Mio | 1 | 5 sur 5 | 0 |
| 384 Mio | 2 | 5 sur 5 | 0 |
| 512 Mio | 4 | 5 sur 5 | 0 |

À chaque démarrage nominal, le firmware a effectué **6 lectures SATA (4 608 octets) avant l’entrée EFI**. Ensuite, les compteurs QMP de lecture, écriture et FLUSH sont restés stables et le contenu du témoin a gardé son empreinte. La trace PCI enregistre deux réécritures xHCI de la même valeur héritée pendant la chaîne de chargement. Le diagnostic noyau s’arrête avant tout pilote réseau ou stockage. Le banc mesure le disque émulé.

Chaque parcours nominal couvre 1 182 pages noyau de 4 Kio, dont 218 de code RO/X, 945 de constantes RO/NX, 1 de requêtes RO/NX et 18 de données RW/NX. CR0.WP et EFER.NXE sont actifs après installation des tables noyau. Les 1 000 pages de la surface sont contiguës, dédiées au framebuffer dans la carte Limine et inscriptibles uniquement en superviseur. **Sous les tables héritées de Limine, ces 1 000 pages permettent l’exécution. Le noyau les remappe toutes NX.** Ces vérifications portent sur les traductions CPU. La protection DMA préalable sur un PC physique reste indéterminée.

## Refus et absence d’écran

| Cas exercé à 384 Mio et 1 vCPU | Résultat |
|---|---|
| Adresse du noyau absente du relais Limine | R01, motif visible avant ordonnanceur |
| Mémoire du noyau refusée | R02, motif visible avant ordonnanceur |
| Scénario de démarrage non admis | R03, motif visible avant ordonnanceur |
| Topologie MP vide | R04, motif visible avant ordonnanceur |
| Surface retirée du relais, VGA toujours présent | R06 en RAM et série après SMP. Le scanout reste identique à celui d’avant l’entrée du noyau. |
| Aucune carte VGA émulée | R06 en RAM et série après SMP. QEMU confirme l’indisponibilité du scanout. |
| Quatre supports altérés séparément | Le premier EFI refuse le manifeste, Limine, sa configuration ou le noyau avant exécution du noyau. |

Les six cas de panne noyau utilisent l’image nominale et une modification du relais en RAM ou de la topologie virtuelle. Les quatre altérations de support utilisent des images synthétiques distinctes. Les dix essais aboutissent à l’arrêt local et à des compteurs SATA stables depuis l’entrée EFI. Le code R05 couvre une panne après lancement de l’ordonnanceur ou du rendez-vous SMP. Son injection dédiée reste à qualifier. Une panne antérieure à une surface exploitable peut laisser l’écran figé ou noir. [Capture du refus mémoire](validation/live-noyau-6a/refus-memoire.png)

## Nettoyage du volume de développement

La compilation et les démarrages 6A étaient terminés lors de l’alerte disque. Leur reprise est passée par la relecture des preuves et des octets livrés. Le contrôle des processus a permis de traiter les anciens artefacts sans interrompre de campagne tierce.

| Périmètre vérifié | Traitement |
|---|---|
| 4 859 copies de banques firmware émulées | 294,19 Gio de blocs nuls désalloués. Fichiers conservés à leur emplacement, taille et SHA-256 vérifiés, références intactes. |
| 97 209 intermédiaires Cargo | 38,40 Gio de bibliothèques intermédiaires et de métadonnées de dépendances verrouillées retirés de caches identifiés. |
| Deux campagnes de tri terminées | 10 fichiers de fusion temporaires, soit 19,77 Gio. Format synthétique contrôlé sur chaque enregistrement, résultats historiques conservés. Deux répertoires temporaires clos. |
| Worktrees antérieurs | 5T retiré après vérification du SHA distant, de l’état propre, des fichiers ignorés et des dépendances. Le receveur de tri sous `tmp` reste utilisé comme référence et contient deux ELF modifiés. |
| NovaAuth R2 suspendu | Sources, résultats, archives privées, exécutables et fichiers de reprise conservés. La revue automatique a bloqué un lot supplémentaire de 102 166 intermédiaires temporaires pour lequel l’indépendance vis-à-vis de R2 était insuffisamment établie. Ce lot est resté en place. |

Le cumul des blocs des fichiers traités atteint **352,36 Gio**. Le répertoire principal `novaOS/build` est passé de **583,71 à 262,73 Gio** selon `du`. Le volume est passé d’environ **1,6 à 88,5 Gio immédiatement disponibles**, soit un gain d’environ **87 Gio** selon `df`. Un instantané Time Machine local purgeable a été observé pendant la sauvegarde. Les blocs partagés ou conservés par APFS expliquent que le cumul des fichiers dépasse le gain immédiatement visible. La réserve de 30 Gio est rétablie. Le temporaire partagé représente encore environ 198 Gio accessibles, dont des éléments de chantiers suspendus.

La règle commune impose la gestion des échecs, l’archivage durable des preuves indispensables, la vérification des dépendances avant retrait des worktrees et la mesure du volume à la clôture. L’image brute qualifiée et les reçus détaillés du nettoyage disposent d’une conservation privée hors de `tmp`. [Consignes communes NovaOS et NovaFS](../../AGENTS.md#clôture-des-campagnes-codexdev-pour-novaos-et-novafs)

## Non-régression et portée physique

Les scénarios ABI et mémoire du noyau ordinaire ont réussi à 256 Mio sous QEMU **AArch64 et x86_64**. Le contrôle de l’ABI confirme les 48 sélecteurs. Sur Darwin ARM64 natif et x86_64 sous Rosetta, la contre-lecture des preuves historiques STANDARD a validé cinq cellules et le refus 17 sans console. Celle du profil RENFORCÉ couvre quatre cellules 5R et les deux modes 5S. Les suites d’image 5N et 5O ont passé neuf et six tests sur chaque architecture hôte. Les démarrages de ces profils proviennent de leurs campagnes d’origine. [Reproduction et contre-lecture](../boot/live-6a/README.md) · [Preuves ABI des deux invités](validation/live-noyau-6a/regression-abi.tar.gz) · [Registre](validation/qualifications-demarrage.md)

Le deuxième essai physique communiqué par la MOA concerne exclusivement l’inventaire EFI STANDARD du Lenovo. Le noyau 6A a été exécuté uniquement sous QEMU. La candidature MSI distingue les risques historiques des risques supplémentaires de transition mémoire, de topologie SMP et de surface graphique. [Relevé Lenovo](diagnostic-standard-lenovo.md) · [Candidature A8](candidature-a8-noyau-6a.md)
