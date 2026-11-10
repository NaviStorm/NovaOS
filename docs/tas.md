# Tas noyau de NovaOS

Après la prise de contrôle de la MMU, le noyau installe son allocateur global. Il possède le gestionnaire de mémoire et obtient ses pages superviseur RW/NX par `Memory::allocate_page`. Il les rend par `Memory::release_page`. L’arbre de blocs et les objets Rust utilisent uniquement ces interfaces. La HAL intervient dans le verrou pour sauvegarder, masquer et restaurer les interruptions locales.

## Allouer depuis Rust

Le binaire reste `no_std` et importe la crate `alloc` fournie avec la cible Rust stable. `Box`, `Vec` et `String` utilisent l’instance marquée `#[global_allocator]`. Dans le chemin interactif x86_64, chaque ligne réserve d’avance ses 128 caractères. Entrée remet la ligne à l’appelant et sa destruction restitue le bloc.

Pour une demande susceptible d’épuiser la mémoire, `Vec::try_reserve_exact` et `String::try_reserve` permettent au client de reprendre la main. Au niveau de `GlobalAlloc`, une pénurie renvoie un pointeur nul. Un échec de réallocation préserve l’adresse, la disposition et le contenu précédents. Une collection utilisée via une opération infaillible suit le traitement standard de Rust, qui déclenche la panique terminale du noyau. [Contrat de GlobalAlloc](https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html) · [Traitement des erreurs d’allocation](https://doc.rust-lang.org/alloc/alloc/fn.handle_alloc_error.html)

## Organisation et coûts

La fenêtre virtuelle des objets commence à `0xffff_f001_0000_0000` et mesure 256 Mio. Chaque bloc est une puissance de deux d’au moins 32 octets. Sa taille couvre la charge demandée, 24 octets de contrôle et l’alignement requis. La base de la fenêtre est alignée sur sa capacité.

La longueur engagée se décompose en arbres de blocs jumeaux. Chaque nœud divisé indique la taille du plus grand bloc libre de son sous-arbre. L’allocation suit cette indication, divise un bloc libre puis actualise ses ancêtres. La libération fusionne les jumeaux libres. La recherche et la mise à jour sur des pages déjà engagées sont logarithmiques, avec au plus 23 niveaux de division. Une réallocation privilégie le redimensionnement sur place, y compris par extension en fin de tas. Lors d’un déplacement, la copie des octets utiles s’effectue hors verrou. La primitive interne `heap::realign` accepte aussi un nouvel alignement.

Les indicateurs de l’arbre résident dans une fenêtre privée distincte. Ils coûtent deux octets par tranche de 32 octets engagée, arrondis à des pages de 4 Kio. Le noyau mappe les nouvelles pages de métadonnées et de données avant de publier une extension. Si le dernier cadre manque, il démape les pages provisoires dans l’ordre inverse. Quand le suffixe libre permet de rendre au moins 64 Kio, le tas le restitue à la MMU et réduit aussi ses métadonnées. En fonctionnement courant, un tas ayant servi conserve au moins une page de données en réserve. La sonde peut purger ce cache pour comparer deux bilans à charge identique.

## Contrôles et limites

- L’arbre confirme que le pointeur désigne le début exact d’un bloc vivant avant toute lecture de sa balise. Une seconde libération ou une adresse intérieure est rejetée. Une balise en fin de bloc mémorise la taille et l’alignement demandés. Une sentinelle de huit octets, immédiatement après la charge utile, est vérifiée avant réallocation ou libération.
- La page précédant la fenêtre et celle qui suit les données engagées sont absentes des tables. La garde haute se déplace avec le tas. Un accès à cette garde provoque une faute matérielle. Une corruption détectée par l’allocateur global produit un diagnostic série et un arrêt contrôlé.
- La conversion d’un `Layout` en taille de bloc et le calcul d’une extension utilisent des opérations arithmétiques vérifiées. Les indices internes sont bornés par la fenêtre de 256 Mio. L’arrondi en puissances de deux et les grands alignements peuvent laisser de l’espace inutilisé. Des trous entre objets vivants restent réutilisables dans le tas mais peuvent empêcher une grande demande contiguë.
- Les contrôles sont diagnostiques. Une écriture hors limites qui saute la sentinelle et la garde peut échapper à la détection. Une ancienne adresse recyclée pour une nouvelle allocation de même disposition est indiscernable à partir du seul pointeur. Le client Rust doit respecter le contrat de propriété de l’allocateur.
- Le verrou atomique utilise les ordres acquire/release. Les interruptions locales sont masquées avant sa prise et restaurées après sa libération. Les parcours, les opérations de pagination et le journal d’erreur utilisent la pile et des données préallouées. Le mappage ou la restitution d’une grande plage maintient les interruptions masquées pendant un temps proportionnel au nombre de pages.
- Chaque instance décrit sa base, sa limite et ses propres métadonnées. La v0.4.0 installe uniquement le tas superviseur. Les futurs tas de processus nécessiteront un espace d’adressage utilisateur et une politique de quotas. Les tests couvrent QEMU TCG mono-CPU. La contention SMP, les invalidations TLB distantes et la latence sur matériel physique restent à valider.

## Sondes exécutées sous QEMU

```sh
make ARCH=x86_64 test
make ARCH=aarch64 test
```

| Scénario | Vérification |
| --- | --- |
| `heap-test` | Libérations alternées sur 128 blocs pendant huit cycles, réutilisation des adresses, fusion, remise à zéro, 1 500 opérations mixtes, alignements de 1 octet à 2 Mio, réallocation sur place ou avec déplacement. Les pointeurs de `Box`, `Vec` et `String` sont retrouvés dans l’arbre. Les erreurs de pointeur, de disposition, de taille et la double libération sont contrôlées par le moteur interne. |
| `heap-oom` | Une fenêtre de test immobilise de vrais cadres jusqu’à ce qu’il en reste au plus huit. Une réservation de vecteur et la croissance d’un bloc existant échouent. Le contenu, le nombre de cadres libres et la longueur engagée sont contrôlés. Une fois les cadres rendus, les mêmes demandes réussissent. |
| `heap-corrupt` | La sonde altère sa propre sentinelle. La libération par le chemin global produit le diagnostic `Overrun` et la VM entre en veille. |
| `heap-metadata` | La sonde masque un bloc vivant dans l’arbre. Le décompte indépendant détecte la corruption et produit `Metadata`. |
| `heap-reentrant` | Une acquisition imbriquée sur le même CPU produit `Reentrant` et un arrêt contrôlé. |
| `heap-guard` | Une lecture de la garde haute provoque une faute de traduction. Le test confronte l’adresse et le syndrome à l’accès demandé. |
| `heap-infallible` | Une capacité supérieure à la fenêtre est demandée à `Vec::with_capacity`. La panique d’allocation est journalisée et le CPU entre en veille. |

Les sondes terminales démarrent chacune dans une VM distincte. Les tests d’affichage, de saisie AZERTY et de protection des pages de la v0.3.0 font toujours partie des suites.
