# Extension IPC proposée de l’ABI native v1

**Statut — proposition pour la v0.8.0, base `4900cf5`.** Sur cette base, pour un contexte de retour utilisateur valide, les sélecteurs IPC ci-dessous renvoient `UNKNOWN_CALL`. Les six sélecteurs publiés `0x10001` à `0x10006`, leurs arguments, leurs erreurs, `Reply` et `WaitInfo` conservent leur contrat. `process_wait` continue de consommer son droit de suivi spécialisé. [Constantes et types livrés](../abi/native.rs) · [Dispatch livré](../kernel/src/sched/process/syscall.rs) · [ABI native v1](abi.md)

## Porte et représentation

| Cible | Instruction | Sélecteur u64 | a, b, c (u64) | statut, valeur (u64) |
| --- | --- | --- | --- | --- |
| x86_64, Ring 3 | `syscall` | RAX | RDI, RSI, RDX | RAX, RDX |
| AArch64, EL0t | `svc #0x4e` | x8 | x0, x1, x2 | x0, x1 |

Les règles v1 de préservation des registres, de validation du PC/SP et de préemption s’appliquent. Sur x86, RCX et R11 sont détruits par SYSCALL. Les autres GPR hors résultats et RSP suivent la préservation v1, avec SYSRET/IRETQ. Sur ARM, x2 à x30, SP_EL0, TPIDR_EL0 et NZCV sont préservés, avec retour ERET. Les structures utilisateur sont little-endian. Un pointeur est transmis dans un registre u64, avec alignement de 8 octets pour chacune des trois structures. Les corps contiennent exclusivement des entiers de taille fixe et des octets en ligne. Les identités, droits et délais sont des valeurs scalaires.

Le sélecteur a la forme `(1 << 16) | opération`. Dans le tableau, « — » désigne un argument inutilisé, ignoré comme dans l’ABI native. Un émetteur peut le mettre à zéro. Les **champs réservés des structures**, eux, doivent valoir zéro. Pour chaque descripteur, `version=1`, `size` et l’argument c doivent correspondre exactement à la taille indiquée. Une forme ultérieure incompatible demandera de nouveaux sélecteurs ou une version d’ABI distincte.

## Sélecteurs proposés

| Sélecteur | Appel | a | b | c | Droit sur a | Valeur sur succès |
| --- | --- | --- | --- | --- | --- | --- |
| `0x10020` | `ipc_create` | — | — | — | Création soumise aux quotas | Handle racine neuf |
| `0x10021` | `ipc_dup` | Handle | Masque u64 | — | GRANT | Handle enfant neuf dans la table locale |
| `0x10022` | `ipc_close` | Handle | — | — | Possession | 0. Fermer la racine détruit l’endpoint |
| `0x10023` | `ipc_revoke` | Handle | — | — | MANAGE | Nombre de handles vivants de descendants invalidés, ancêtres structurels exclus |
| `0x10024` | `ipc_send` | Canal | Adresse de `SendV1` | 160 | SEND | Nombre d’octets acceptés, y compris zéro |
| `0x10025` | `ipc_receive` | Canal | Adresse de `ReceiveV1` | 184 | RECEIVE | `kind` livré, identique au champ de sortie |
| `0x10026` | `ipc_give` | Canal | Adresse de `GiveV1` | 48 | SEND et GRANT | 0 après installation chez le receveur |
| `0x10027` | `ipc_notify` | Canal | Masque de bits u64 non nul | — | SEND | 0 après fusion |
| `0x10028` | `ipc_cancel` | Handle de canal | — | — | MANAGE | Nombre d’appels passés à CANCELLED dans la branche, zéro si vide |
| `0x10029` | `ipc_bootstrap` | — | — | — | Alias local installé par le lanceur | Handle local existant. DENIED si l’alias est vide ou périmé |

Le masque valide des droits est `0xF`. SEND vaut 1, RECEIVE 2, GRANT 4 et MANAGE 8. Pour une dérivation, un masque nul ou comportant un bit inconnu donne BAD_ARGUMENT. Un droit connu absent de la source donne DENIED. Pour `ipc_dup`, ceci s’applique aux 64 bits de b. [Arbre et durée de vie](ipc-architecture.md)

## Dispositions binaires

Les offsets sont exprimés en octets depuis le pointeur b. Les tableaux ci-dessous énumèrent l’intégralité des octets. Les longueurs et extensions ne dépendent ni de `usize`, ni d’un enum Rust, ni d’une adresse de fonction.

### SendV1 — 160 octets, alignement 8, entrée

| Offset | Type | Champ | Contrat |
| ---: | --- | --- | --- |
| 0 | u32 | version | 1 |
| 4 | u32 | size | 160 |
| 8 | u64 | timeout | Délai relatif en ticks |
| 16 | u64 | tag | Libre, copié à la réception |
| 24 | u32 | len | 0 à 128 inclus |
| 28 | u32 | flags | 0 = asynchrone, 1 = synchrone |
| 32 | u8[128] | payload | Seuls les `len` premiers octets ont un sens applicatif |

Le noyau lit une fois la structure entière, même pour un message vide. Les octets après `len` sont ignorés et la sortie correspondante est mise à zéro. Asynchrone exige `timeout=0`. Synchrone exige `1..15000`. Une autre combinaison donne BAD_ARGUMENT. Un dépôt asynchrone en file pleine donne WOULD_BLOCK. Un succès synchrone signifie que la livraison a été validée par un receveur.

### ReceiveV1 — 184 octets, alignement 8, entrée/sortie

| Offset | Type | Champ | Contrat |
| ---: | --- | --- | --- |
| 0 | u32 | version | 1 |
| 4 | u32 | size | 184 |
| 8 | u64 | timeout | 0 pour un essai, sinon 1 à 15 000 |
| 16 | u32 | options | Bit 0 = ACCEPT_CAP. Les autres bits sont réservés |
| 20 | u32 | reserved | 0 |
| 24 | u32 | kind | Sortie. DATA=1, NOTIFY=2, CAP=3 |
| 28 | u32 | len | Sortie. Longueur utile de DATA, sinon zéro |
| 32 | u64 | tag | Sortie. Tag de DATA ou de CAP, zéro pour NOTIFY |
| 40 | u64 | handle | Sortie. Handle local neuf pour CAP, sinon zéro |
| 48 | u64 | aux | Sortie. Bits fusionnés pour NOTIFY, masque des droits pour CAP, zéro pour DATA |
| 56 | u8[128] | payload | Sortie. Contenu de DATA suivi de zéros. Tous les octets sont nuls pour NOTIFY et CAP |

Le noyau copie le préfixe d’entrée de 24 octets et valide l’écriture de **184 octets** avant d’inscrire une attente. Les anciens champs de sortie sont ignorés. Au succès, il réécrit le préfixe à partir de sa copie validée et initialise entièrement le reste. Une erreur laisse les 184 octets intacts du point de vue du noyau. Le client doit synchroniser ses propres accès au tampon pendant l’appel. Une longueur utile de zéro est permise pour DATA et n’encode pas une notification.

### GiveV1 — 48 octets, alignement 8, entrée

| Offset | Type | Champ | Contrat |
| ---: | --- | --- | --- |
| 0 | u32 | version | 1 |
| 4 | u32 | size | 48 |
| 8 | u64 | timeout | 1 à 15 000 ticks |
| 16 | u64 | source | Handle local valide avec GRANT |
| 24 | u64 | tag | Donnée libre de l’émetteur |
| 32 | u32 | rights | Masque non nul des droits transmis |
| 36 | u32 | mode | COPY=1, MOVE=2 |
| 40 | u64 | reserved | 0 |

COPY donne un descendant avec les droits demandés. MOVE exige un nœud non racine et tous ses droits. Un MOVE de racine donne DENIED. Une atténuation lors de MOVE ou l’utilisation de la même entrée comme canal et comme source de MOVE donne BAD_ARGUMENT. Des handles distincts vers le même endpoint sont permis. Le receveur doit fournir ACCEPT_CAP et un slot libre, également pour un déplacement entre deux threads d’un même processus. La source reste utilisable pendant l’attente et est revalidée au commit. Une course perdue contre un autre déplacement donne REVOKED au don encore en attente.

## Statuts et priorité des contrôles

`Reply.status=0` signifie succès. Pour **toute** erreur, `Reply.value=0`. Aucun compteur ni adresse partielle ne sont retournés sur erreur.

| Code u64 | Statut | Application |
| ---: | --- | --- |
| 0 | OK | Succès au point d’effet défini pour l’appel |
| 1 | BAD_VERSION | Version haute du sélecteur ou version d’une nouvelle structure IPC inconnue |
| 2 | UNKNOWN_CALL | Opération inconnue pour une version de sélecteur reconnue |
| 3 | BAD_ARGUMENT | Taille, alignement, bit réservé, masque, longueur, mode ou délai hors contrat |
| 4 | BAD_ADDRESS | Plage débordante, noyau, absente ou de permissions insuffisantes |
| 5 | DENIED | Handle absent, étranger, périmé, de mauvais type ou de droits insuffisants. Même réponse pour un ancien handle après destruction |
| 6 | LIMIT | Quota, profondeur, table destinataire, budget physique IPC, identifiants ou horizon de temps épuisés |
| 7 | WOULD_BLOCK | FIFO asynchrone pleine ou réception immédiate sans élément attribuable |
| 8 | TIMED_OUT | Échéance positive atteinte avant la livraison |
| 9 | CANCELLED | Opération en attente annulée par un gestionnaire autorisé |
| 10 | REVOKED | Dépendance d’une opération déjà admise invalidée |
| 11 | CLOSED | Endpoint de canal détruit pendant l’attente |
| 12 | REFUSED | Don présenté au receveur de tête qui a omis ACCEPT_CAP. Résultat pour les deux appels |
| 13 | NO_MEMORY | Échec réel d’une allocation nécessaire dans les quotas |

Les codes 0 à 6 existent déjà. Les codes 7 à 13 et l’usage de BAD_VERSION pour les nouvelles structures sont proposés uniquement pour l’IPC. Les versions et opérations inconnues sur la porte native conservent leur comportement actuel.

**Ordre d’entrée.** Vérifier le contexte de retour selon la v1, puis décoder la version et l’opération. Pour un appel sur handle, résoudre le handle primaire et ses droits dans la table de l’appelant avant d’accéder à la mémoire utilisateur. Pour un descripteur, vérifier d’abord c et l’alignement, puis la plage et la copie d’entrée, puis la version, la taille interne, les champs et combinaisons réservés. Pour receive, la validation initiale de la plage inclut son caractère inscriptible en entier. Pour give, résoudre ensuite la source, vérifier les droits demandés et les contraintes de MOVE. Réitérer les contrôles d’autorité après une préparation qui a relâché la protection. Une erreur de contexte de retour termine le seul processus conformément à la v1.

**Ordre d’admission et de commit.** Traiter les expirations dues. Une FIFO pleine ou un essai de réception qui prendrait la place d’un receveur plus ancien donne WOULD_BLOCK. Les limites logiques applicables précèdent les demandes d’allocation physique. Si la situation change avant publication, réévaluer la décision et restituer les réservations inutiles. À la livraison, vérifier que les dépendances de la tête et du receveur restent actives, puis revalider la sortie complète sous protection mémoire. Une mauvaise sortie donne BAD_ADDRESS au receveur et conserve la tête. Pour un don encore présent, appliquer ensuite le refus explicite, puis le quota de slots, puis le commit. Un résultat terminal déjà acquis est définitif. L’ordre des courses est détaillé dans la spécification de sécurité. [Points d’effet](ipc-security.md)

## Vecteurs de référence

Le préfixe de 32 octets suivant décrit un SendV1 asynchrone de longueur 3 et de tag `0x0102030405060708`. Les trois octets suivants valent `41 00 ff`. Les 125 derniers octets d’entrée peuvent contenir n’importe quelle valeur.

```text
01 00 00 00 a0 00 00 00 00 00 00 00 00 00 00 00
08 07 06 05 04 03 02 01 03 00 00 00 00 00 00 00
```

Pour une réception immédiate avec options zéro, le même message produit le préfixe suivant de 56 octets, suivi de `41 00 ff` puis 125 zéros. Le couple de retour est `(0,1)`.

```text
01 00 00 00 b8 00 00 00 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00 01 00 00 00 03 00 00 00
08 07 06 05 04 03 02 01 00 00 00 00 00 00 00 00
00 00 00 00 00 00 00 00
```

Deux autres oracles utilisent le même tampon de réception de 184 octets, entièrement initialisé à zéro avant encodage des champs indiqués.

| Livraison | Champs non nuls (offset, valeur little-endian) | Retour |
| --- | --- | --- |
| NOTIFY après les masques 1 et 4 | u32(0)=1, u32(4)=184, u32(24)=2, u64(48)=5 | (0,2) |
| CAP acceptée, tag 9 et droits SEND | u32(0)=1, u32(4)=184, u32(16)=1, u32(24)=3, u64(32)=9, u64(40)=H, u64(48)=1 | (0,3) |

H désigne le nouveau handle observé chez le receveur. Vérifier qu’il est non nul, distinct du handle source et utilisable pour SEND. Pour obtenir le second cas, le receveur peut avoir fait un **essai immédiat** avec ACCEPT_CAP sur une offre déjà inscrite. Une entrée GiveV1 correspondante a pour champs (version, size, timeout, source, tag, rights, mode, reserved) les valeurs `(1,48,3,S,9,1,1,0)`, où S est une source détenue avec SEND et GRANT.

Le refus d’une clé étrangère par `ipc_send` retourne `(5,0)`, même si b vise une adresse noyau. Un SendV1 de longueur 129 sur un canal autorisé retourne `(3,0)`. Avec une sortie alignée qui chevauche une garde, `ipc_receive` retourne `(4,0)` et laisse l’élément disponible.

**Contraintes de migration.** Le plus grand transfert structurel est de 184 octets, compatible avec la borne de copie actuelle de 256 octets sur deux pages. La v1 ne propose ni bibliothèque IPC, ni allocation de handle générique, ni primitive qui effectue à elle seule un commit couplé entre mémoire et table de capacités. Ces dernières étapes devront être ajoutées au noyau commun. Le dispatcher v0.7.0 devra continuer à reconnaître les six sélecteurs existants exactement comme auparavant. [Copies actuelles](../kernel/src/memory/user.rs) · [Validation proposée](ipc-test-plan.md)
