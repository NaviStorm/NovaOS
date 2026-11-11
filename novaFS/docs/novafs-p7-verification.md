# Vérificateurs et primitives du format expérimental P7

## Vérifier une image hors ligne

Le vérificateur attend un fichier ordinaire sur son entrée standard. `BLOCS` désigne le nombre de blocs de 4 Kio dans l'image et `SECTEUR` vaut 512 ou 4096. Les commandes historiques P0, P5 et P6 conservent leurs syntaxes.

```sh
novafs check-pool BLOCS SECTEUR < pool.raw
novafs check BLOCS SECTEUR POOL_ID OBJET EMPREINTE < pool.raw
```

Sans secret, le programme compare les deux en-têtes, examine la forme de chaque attestation présente et vérifie la relation entre les deux dernières générations déclarées. Il contrôle les empreintes des banques, la forme des répertoires et des emplacements, les références vers les racines, les en-têtes et les empreintes des paires correspondantes. Les vues claires permettent en outre le parcours de leur graphe. Les champs de la racine chiffrée restent opaques. Un attaquant peut recomposer toutes les empreintes publiques. Le rapport JSON indique `authenticite_verifiee=false` et termine avec le code 3 pour un résultat structurel sans incohérence détectée. Le champ `copies_protegees_redondantes` indique si la tête déclarée et son prédécesseur possèdent chacun deux copies canoniques concordantes. Un reçu cible localise le pool et la vue dans ce mode.

Le mode complet reçoit un canal distinct. Sous zsh sur macOS, un producteur de secrets de confiance peut émettre directement la trame binaire suivante sur un pipe. `producteur` représente ici le programme du détenteur des clés. Le test `p7_cli` fournit un exemple exécutable utilisant des données synthétiques et un vrai pipe anonyme.

```sh
producteur | (exec 3<&0; exec novafs check-pool BLOCS SECTEUR --cles-fd3 < pool.raw)
producteur | (exec 3<&0; exec novafs check BLOCS SECTEUR POOL_ID OBJET EMPREINTE --cles-fd3 < pool.raw)
```

La trame `NP7KEYS1` comporte les champs suivants, dans l'ordre, sans bourrage.

| Champ | Encodage |
| --- | --- |
| Magie | Huit octets ASCII `NP7KEYS1` |
| Pool attendu | 16 octets d'identité provisionnés indépendamment de l'image |
| Clé administrative | 32 octets |
| Nombre de domaines fournis | Un octet, entre 0 et 17 |
| Chaque enregistrement | Domaine non nul sur 16 octets, type sur un octet, longueur sur deux octets non signés little-endian, puis le secret |
| Type 1 | Octets de la phrase tels que provisionnés, entre 1 et 1024 inclus |
| Type 2 | Clé de récupération binaire de 32 octets |
| Fin | EOF immédiat après le dernier enregistrement |

La taille maximale atteint 17 788 octets. Les domaines répétés, les types inconnus, la troncature et un octet en excès donnent une erreur d'utilisation. L'adaptateur hôte accepte un pipe anonyme Darwin sur le descripteur 3. Il contrôle le type FIFO, l'identifiant de périphérique nul et le nombre de liens nul. Le code cœur de vérification reçoit des références de clés Rust. Les tampons contenant la phrase et les clés sont effacés à leur libération. Le producteur reste responsable de ses propres copies, de son terminal et de ses journaux.

Le contrôle complet authentifie la clé administrative via l'enveloppe du pool, vérifie chaque copie non vide d'attestation, déverrouille les domaines fournis puis exécute le contrôle intégral des métadonnées et du contenu. Le contrôle ciblé compare en plus le reçu externe. Le code 1 signale un MAC incorrect ou une copie manquante ou divergente de la tête ou du prédécesseur. La toute première génération requiert uniquement ses deux propres copies. Le montage de récupération peut conserver la génération grâce à la copie survivante. La perte des deux copies de la tête peut rendre une génération antérieure indiscernable de la dernière génération durable sans témoin externe. Le code 0 accompagne la vérification complète réussie. Les codes 1, 2 et 3 indiquent respectivement une incohérence ou un échec d'authentification, un usage invalide et une analyse incomplète. Le reçu externe du client et une autorité de fraîcheur restent distincts.

## Dépendances cryptographiques contrôlées

La clé maîtresse, les sous-clés, les phrases détenues par le lecteur de trame et les 65 536 blocs de travail Argon2 sont effacés explicitement. Les sous-clés de 32 octets restent à adresse fixe pendant leur possession. Le laboratoire utilise un calcul Argon2id à la fois, m = 65 536 Kio, t = 3, p = 4, v = 0x13. Le format refuse tout autre profil avant d'allouer le tampon de dérivation. L'effacement des copies du compilateur, des registres, du swap ou des tampons de bibliothèques hôtes échappe au contrôle du programme.

| Code importé par le cœur P7 | Version verrouillée et contrôle |
| --- | --- |
| RustCrypto argon2 | 0.6.0 avec `zeroize`. Matrice de travail détenue par NovaFS |
| RustCrypto chacha20poly1305 | 0.11.0 avec `zeroize`. Comparaison du tag par la bibliothèque avant exposition du clair |
| RustCrypto poly1305 | 0.9.1, activation explicite de `zeroize` sur cet état interne |
| RustCrypto hkdf, hmac | 0.13.0, HMAC avec `zeroize`. Vérification constante par la bibliothèque |
| RustCrypto sha2 | 0.11.1 avec `zeroize` pour HKDF et HMAC. Les sommes publiques historiques utilisent 0.10.9 |
| zeroize | 1.9.1 avec `alloc` |
| getrandom | 0.4.3, adaptateur hôte derrière `Entropy` |

Le contrôle des licences repose sur les manifestes des dépendances verrouillées. Les nouvelles dépendances cryptographiques autorisent MIT ou Apache-2.0. Le registre officiel RustSec consulté à la révision `28d73fc95e5972af3403225a217def57dcaba3c4` contient des avis pour blake2, chacha20, cmov, ctutils, generic-array, once_cell et sha2 présents dans le verrou. Chacune des versions utilisées se situe dans la plage corrigée publiée. Cet examen des avis connus ne constitue pas un audit indépendant des sources. [Base RustSec consultée](https://github.com/RustSec/advisory-db/tree/28d73fc95e5972af3403225a217def57dcaba3c4) · [Implémentations RustCrypto](https://github.com/RustCrypto)

Les quatre tests de primitives comparent notamment XChaCha20-Poly1305 à un vecteur libsodium 1.0.22, Argon2id à un vecteur OpenSSL 3.5.9 et HMAC/HKDF à des résultats de la bibliothèque Python. Les tests du format d'unité exécutent l'authentification avant lecture du répertoire LZ4, y compris lorsque l'empreinte publique a été recomposée après modification. Le banc teste également le refus d'une substitution de rôle, de domaine, de génération ou de position.

```sh
python3 novaFS/lab/p7-no-std.py
for cible in aarch64-apple-darwin x86_64-apple-darwin; do
    NOVAFS_TARGET="$cible" novaFS/lab/run test --offline --lib crypto::tests -- --test-threads=1
    NOVAFS_TARGET="$cible" novaFS/lab/run test --offline --lib secure_unit::tests -- --test-threads=1
    NOVAFS_TARGET="$cible" novaFS/lab/run test --offline --test p7_cli -- --test-threads=1
done
```

Le banc bare-metal compile le fichier de primitives effectivement utilisé, les mêmes dépendances et un type d'erreur minimal. Les cibles `x86_64-unknown-none` et `aarch64-unknown-none-softfloat` doivent être installées dans la chaîne Rust stable. Sur ARM64 softfloat, le banc sélectionne le backend SHA logiciel pour respecter l'ABI sans NEON. Le test ne démarre aucun noyau.
