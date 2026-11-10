# Mémoire virtuelle de NovaOS v0.3.0

Le noyau construit un espace superviseur à quatre niveaux avant d’initialiser le framebuffer. Chaque table et chaque page occupent 4 Kio. Le démarrage utilise un seul processeur, interruptions masquées. Le code commun interprète la carte mémoire et gère les cadres. La HAL encode les descripteurs propres au processeur, installe les racines et synchronise les caches de traduction.

## Prise de contrôle depuis Limine

La révision de base 3 fournit le décalage du mappage direct en moitié haute (HHDM), les adresses physique et virtuelle du noyau, la carte physique et le mode de pagination. NovaOS vérifie les réponses et la géométrie avant d’utiliser les cadres. Il accède à une table physique via son adresse HHDM. Pendant la reprise, il suit aussi les grands blocs des tables héritées pour retrouver le cadre et les attributs de cache de chaque page. [Protocole Limine](https://github.com/limine-bootloader/limine-protocol/blob/trunk/PROTOCOL.md)

L’allocateur parcourt exclusivement les plages `USABLE` alignées, en excluant le cadre zéro. Il conserve son curseur et une liste de cadres restitués dont le premier mot contient le lien suivant. Chaque attribution remet le cadre à zéro. Ces métadonnées n’exigent ni tas, ni tableau dimensionné par la plus grande adresse physique.

La nouvelle carte directe couvre la RAM utilisable, la mémoire du chargeur, l’image et le framebuffer. La mémoire du chargeur reste réservée, notamment la pile, les réponses Limine, la GDT x86 et ses anciennes tables. Les plages ACPI, réservées ou défectueuses restent hors de l’allocateur. Le PL011 de la machine ARM virt dispose de son propre mappage de périphérique.

## Permissions par page

| Plage | Vue principale | Alias HHDM |
| --- | --- | --- |
| Requêtes Limine après démarrage | Lecture | Lecture |
| Code `.text` | Lecture et exécution | Lecture |
| Constantes `.rodata` | Lecture | Lecture |
| Données `.data` et `.bss` | Lecture et écriture | Lecture et écriture |

Chaque vue est superviseur. Seule la colonne principale de la ligne code autorise l’exécution. Le script de liaison sépare les plages sur des limites de page. Après activation, le noyau relit la racine matérielle, les entrées parentes et les feuilles de chaque page ELF et de son alias. Le framebuffer garde les attributs de cache établis par Limine.

## API et durée de vie

`Memory::allocate_page` associe un cadre remis à zéro à une adresse alignée dans la fenêtre noyau de 512 Gio qui commence à `0xffff_f000_0000_0000`. `VirtualMemory::map` permet également à l’appelant de fournir un cadre utilisable déjà acquis exclusivement. L’appelant doit garantir la durée de vie du cadre et de ses références. Les permissions exposées sont lecture seule, lecture/écriture et lecture/exécution. Un alias HHDM d’une page RO ou RX est rendu non inscriptible avant la publication du nouveau mappage.

Une adresse hors fenêtre, un défaut d’alignement, une page occupée et une pénurie ont des erreurs distinctes. Les tables intermédiaires manquantes sont construites à part. En cas de pénurie, le gestionnaire les restitue et rétablit les permissions de l’alias. Un démappage invalide la traduction, récupère les tables devenues vides et rend le cadre physique à l’appelant. `release_page` remet également le cadre à l’allocateur. Une racine par espace permet à une même adresse virtuelle de désigner des cadres différents.

## Mécanismes matériels

- **x86_64.** Le noyau contrôle le mode IA-32e à quatre niveaux et la présence de NXE, active CR0.WP et charge son PML4 dans CR3. La transition évacue également les traductions globales. Les entrées utilisent les bits de présence, d’écriture et NX. Les changements locaux utilisent `invlpg`. Les 256 entrées basses du PML4 actif sont vides. La porte IDT 14 capture CR2, RIP, CS et le code matériel. COM1 précise la nature de l’accès, la cause et le privilège. [Manuel système Intel](https://www.intel.com/content/www/us/en/developer/articles/technical/intel-sdm.html)
- **AArch64.** NovaOS reprend une MMU active à EL1, avec 48 bits virtuels et un granule de 4 Kio. Il conserve MAIR et les attributs de partage et de cache des pages. Les feuilles règlent AF, AP, PXN et UXN. Un court trampoline en moitié basse permet de remplacer TTBR1 par une racine vide, d’invalider les traductions puis de publier la nouvelle racine pendant que le code de transition reste accessible. Après le retour en moitié haute, TTBR0 pointe vers une table vide vérifiée. Les changements utilisent les barrières et TLBI. Pour publier du code, le noyau nettoie le cache de données au point d’unification et invalide le cache d’instructions. [Gestion mémoire Armv8-A](https://developer.arm.com/documentation/101811/latest/)

Sur AArch64, VBAR_EL1 désigne 16 vecteurs alignés. Le gestionnaire fatal prend une pile dédiée et journalise FAR_EL1, ELR_EL1, ESR_EL1 et SPSR_EL1 via le PL011 mappé en mémoire Device. Avant que ce périphérique soit accessible, le dernier diagnostic reste lisible par le débogueur au symbole `NOVA_EARLY_DIAGNOSTIC`. Une exception mémoire fatale laisse la VM en veille pour inspection.

## Validation reproductible

```sh
make ARCH=x86_64 test
make ARCH=aarch64 test
```

Les scénarios amorcent le même ELF que l’image normale. Le banc de test ajoute un argument à Limine sur un CD temporaire et contrôle la sortie du noyau avec QMP et le port série.

| Scénario | Observation exigée |
| --- | --- |
| `mem-test` | Cadres distincts, réutilisation avec effacement, écriture via une VA relue par le HHDM, rejet des demandes invalides et restitution des tables. Deux racines hors ligne portent des traductions indépendantes. Une routine en page RX retourne 42. Une pénurie réelle pendant la construction d’une branche laisse le compteur et l’espace cohérents. |
| `mem-ro` | L’écriture dans `.rodata` déclenche une faute de protection. |
| `mem-alias` | L’écriture dans l’alias HHDM du code déclenche une faute de protection. |
| `mem-page-ro` | Une page allouée dynamiquement en lecture seule refuse une écriture. |
| `mem-nx` | L’appel d’une routine placée dans `.data` déclenche une faute d’exécution. |
| `mem-absent` | Une lecture après démappage d’une page précédemment utilisée déclenche une faute de traduction. |

Les cinq fautes intentionnelles sont terminales et chacune démarre dans sa propre VM. Le test confronte l’adresse annoncée à l’adresse fautive, contrôle le PC et décode le syndrome matériel indépendamment du texte du noyau. Les régressions de framebuffer, clavier x86_64, démarrage sans VGA et veille ARM continuent de s’exécuter.

La validation porte sur QEMU TCG, Q35 et virt/Cortex-A72, avec 256 Mio de RAM et un seul CPU. La v0.6.0 ajoute les racines basses privées et les transitions utilisateur. Les invalidations entre plusieurs CPU restent à développer. [Espaces des processus](processus.md) Le matériel physique et la capture d’une panne ARM antérieure au mappage du PL011 n’ont pas été testés. Les adresses physiques prises en charge occupent au plus 48 bits et les tables NovaOS utilisent exclusivement des feuilles de 4 Kio.
