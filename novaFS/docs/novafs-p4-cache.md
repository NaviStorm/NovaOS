# Cache de blocs P4

Un moteur monté détient un cache de pages propres pour son périphérique et son couple FS/LV. Une entrée contient un bloc validé de 4 Kio. Sa clé associe l’adresse dans le LV, la famille du B+Tree ou le type « données », la naissance et le SHA-256 attendus. Deux montages, même sur des images de mêmes identités, n’échangent aucune entrée. Les chemins de vérification et de remontage P2 parcourent directement le périphérique.

## Budget et restitution

`Engine::set_cache_budget(octets)` fixe le plafond dynamique de cette instance. Zéro désactive le cache. Une demande supérieure au plafond du profil, 2 130 944 octets sur les deux ABI qualifiées, rend `Error::Limit` et conserve la politique antérieure. Le défaut, 1 065 472 octets, donne 256 places comme en P3. Avec un budget d’exactement 1 Mio, 251 pages et leur index tiennent. Au plafond, le cache peut retenir 512 pages.

Pour une capacité de `n > 0` pages, `p` est la puissance de deux supérieure ou égale à `n`. Le répertoire réserve `p` descripteurs de 64 octets et `p` indices de deux octets. Les deux allocations ont une taille puissance de deux. Le petit tableau d’indices est imputé au minimum à 16 octets selon la classe Darwin. Le calcul choisit le plus grand `n` tel que `4096 × n + 64 × p + max(16, 2 × p) ≤ budget`. L’index est alloué au premier remplissage. Chaque page de contenu reçoit ensuite son propre bloc à la demande. Une éviction réutilise l’allocation de la victime.

`cache_stats()` expose les places permises (`capacity_pages`), les pages effectivement retenues (`pages`), les octets de contenu (`payload_bytes`), le coût du répertoire alloué (`metadata_bytes`) et leur somme (`charged_bytes`). Les champs `budget_bytes` et `fixed_bytes` indiquent respectivement le plafond dynamique et l’empreinte fixe dans le moteur, verrou compris. La charge dynamique concerne les allocations demandées et les classes Darwin qualifiées. Un allocateur personnalisé peut consommer des structures de gestion supplémentaires. Les pages résidentes et les caches de macOS sont mesurés au niveau du processus.

```rust
fs.set_cache_budget(128 * 1024)?;
let contenu = fs.read("/notes", 0, 4096)?;
let compte = fs.cache_stats();
assert!(compte.charged_bytes <= compte.budget_bytes);
fs.set_cache_budget(0)?;
assert_eq!(fs.cache_stats().charged_bytes, 0);
```

Réduire ou augmenter la capacité effective libère l’ancien contenu et le répertoire avant de permettre un nouveau remplissage. `reclaim_cache()` rend toutes ces allocations et conserve le budget. Un lecteur en vol ou une lecture ultérieure peut le remplir de nouveau. Pour maintenir la restitution, fixer le budget à zéro. Un échec d’allocation de l’index ou d’une page laisse la lecture rendre les octets déjà validés. La création d’un rapport, les tampons du client et les blocs d’un plan CoW relèvent de leurs propres budgets.

## Index, éviction et cohérence

L’adresse détermine un seau. Les descripteurs de ce seau sont chaînés par des indices de 16 bits. Un hit exige ensuite l’égalité de la clé complète. Les collisions n’ont pas d’effet sur le verdict d’intégrité. Le pire cas de recherche parcourt un seau contenant toutes les pages. À saturation, la politique de seconde chance CLOCK efface le bit de référence des pages récemment consultées puis remplace la première page non référencée. La recherche de victime examine au plus `n + 1` descripteurs. Les compteurs cumulatifs saturants `hits`, `misses`, `evictions`, `clock_steps` et `invalidations` permettent d’observer ces charges. Une lecture qui trouve le cache désactivé compte un miss.

Le moteur lit les métadonnées et données sur un miss, contrôle l’enveloppe ou l’empreinte, puis admet les octets validés. Les nœuds restent sous leur représentation disque et sont décodés à la lecture. Le mutex protège le répertoire et les copies en mémoire. Les E/S et le hachage se déroulent hors de ce verrou. Deux lecteurs peuvent donc manquer simultanément le même bloc. L’admission vérifie à nouveau la clé et garde une seule copie.

Le plan CoW privé ne place aucune donnée sale dans le cache. Lors du remplacement logique d’un fichier, les nouvelles références ont leur propre naissance et leur empreinte. Une ancienne copie qui reste dans le cache ne peut donc satisfaire une lecture de la nouvelle référence. Après le retour de F2, le moteur invalide explicitement les adresses physiques du plan présentes dans l’index avant de rendre la nouvelle tête. Le cache ne modifie ni l’ordre de publication ni les deux barrières P1. Si la publication échoue, la mémoire du cache est libérée et l’instance reste gelée jusqu’au remontage.

## Corruption et limites

Un hit peut rendre les octets **précédemment validés** même si le support a depuis subi un défaut silencieux inconnu. `Engine::verify()` contourne le cache, relit les deux générations et confronte le résultat au reçu du montage. Une corruption ou une erreur d’E/S observée par l’instance la gèle et purge son cache. La réinsertion d’un résultat en vol contrôle l’état sous le même verrou ; une lecture ayant précédé la faute sur le support ne peut pas remplir à nouveau le cache d’une instance déjà déclarée défaillante. La CLI hors ligne `check` a sa propre vue du périphérique. Elle ne notifie pas un moteur distinct. Le modèle conserve l’exigence d’un seul écrivain autorisé sur un support stable.

Les mêmes règles s’appliquent aux appels publics des traits `ReadTree::read` et `View::checked_data`. Une erreur de lecture ou de checksum donne son diagnostic d’origine à la première opération, y compris lorsqu’elle provient d’un appel imbriqué de fichier. Après la purge, une autre lecture rend `Unavailable`. `View::raw` lit les octets bruts directement sur le périphérique, sans authentifier leur checksum. Un échec d’E/S de cette porte gèle également le montage. Une issue de publication déjà indéterminée continue de rendre `UnknownOutcome`.

Les tests ont exercé les deux cibles Darwin et les deux géométries de secteur. Ils couvrent collisions concentrées dans le même seau, permutation de slots après retrait, seuils de 0 à 512 pages, refus de configuration, éviction à une seule place, réemploi physique CoW, F2 en échec, remontage et lecture simultanée avec un audit découvrant une corruption. Dans le banc de pénurie, trois refus ponctuels réels de l’allocateur (seaux, descripteurs, page de 4 Kio) par géométrie laissent la lecture réussir puis le cache se repeupler. Les mesures A/B isolent également les hits et le remplacement CLOCK à capacité de 256 pages. [Tests du cache](../tests/cache_p4.rs) · [Essais de pénurie](../tests/check_oom.rs) · [Banc et résultats P4](novafs-p4-performance.md)

```sh
for arch in aarch64 x86_64; do
    NOVAFS_TARGET="$arch-apple-darwin" novaFS/lab/run test --offline \
        --lib --test cache_p4 --test check_oom --test lifecycle || exit 1
done
```
