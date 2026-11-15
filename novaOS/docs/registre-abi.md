<!-- Généré par scripts/generate_abi.py depuis abi/syscalls.toml. Modifier les sources. -->
# Index de l’ABI native v1

Les numéros hexadécimaux identifient les appels sur les deux architectures. Les trois arguments et les deux résultats occupent chacun un mot de 64 bits, en little-endian. Le premier résultat porte le statut. Le second vaut zéro en cas d’erreur.

| Architecture | Instruction | Numéro | Arguments | Statut, valeur |
|---|---|---|---|---|
| x86_64 | syscall | RAX | RDI, RSI, RDX | RAX, RDX |
| aarch64 | svc #0x4e | x8 | x0, x1, x2 | x0, x1 |

Les noms qualifiés correspondent aux constantes Rust. Le lien noyau mène au traitement de l’appel. Les statuts indiqués couvrent les cas caractéristiques. Le traitement lié précise également les erreurs propagées. La colonne SDK indique une fonction de haut niveau lorsqu’elle existe.

| Numéro | Constante | Groupe / depuis / état | Arguments → valeur | Autorité | Erreurs | Noyau | SDK Rust |
|---|---|---|---|---|---|---|---|
| 0x10001 | native::PROCESS_ID | processus / 0.7.0 / active | — → PID | aucun droit additionnel | — | [call](../kernel/src/sched/process/syscall.rs#L99) | [process::id](../libs/nova-system/src/process.rs#L3) |
| 0x10002 | native::THREAD_ID | thread / 0.7.0 / active | — → génération du thread | aucun droit additionnel | — | [call](../kernel/src/sched/process/syscall.rs#L99) | [thread::id](../libs/nova-system/src/thread.rs#L67) |
| 0x10003 | native::THREAD_YIELD | thread / 0.7.0 / active | — → 0 à la reprise | aucun droit additionnel | — | [call](../kernel/src/sched/process/syscall.rs#L99) | [thread::yield_now](../libs/nova-system/src/thread.rs#L70) |
| 0x10004 | native::PROCESS_EXIT | processus / 0.7.0 / active | code, —, — → sans retour | processus appelant | — | [call](../kernel/src/sched/process/syscall.rs#L99) | [process::exit](../libs/nova-system/src/process.rs#L7) |
| 0x10005 | native::PROCESS_WAIT | processus / 0.7.0 / active | clé, *mut WaitInfo, 32 → 32 | droit de suivi, thread principal | DENIED, BAD_ARGUMENT, BAD_ADDRESS, CANCELLED | [wait](../kernel/src/sched/process/syscall.rs#L39) | [process::wait](../libs/nova-system/src/process.rs#L13) |
| 0x10006 | native::DEBUG_WRITE | diagnostic / 0.7.0 / active | *const octets, longueur, — → octets acceptés | quota de diagnostic | BAD_ARGUMENT, BAD_ADDRESS, LIMIT | [debug_write](../kernel/src/sched/process/syscall.rs#L10) | — |
| 0x10020 | native::IPC_CREATE | IPC / 0.8.0 / active | — → handle racine | quotas locaux | LIMIT, NO_MEMORY, CANCELLED | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | [Endpoint::create](../libs/nova-system/src/ipc.rs#L7) |
| 0x10021 | native::IPC_DUP | IPC / 0.8.0 / active | handle, masque, — → nouveau handle | GRANT, réduction des droits | BAD_ARGUMENT, DENIED, LIMIT, NO_MEMORY, CANCELLED | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | [Endpoint::duplicate](../libs/nova-system/src/ipc.rs#L20) |
| 0x10022 | native::IPC_CLOSE | IPC / 0.8.0 / active | handle, —, — → 0 | possession du handle | DENIED, CANCELLED | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | [Endpoint::close](../libs/nova-system/src/ipc.rs#L53) |
| 0x10023 | native::IPC_REVOKE | IPC / 0.8.0 / active | handle, —, — → descendants invalidés | MANAGE | DENIED, CANCELLED | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | [Endpoint::revoke](../libs/nova-system/src/ipc.rs#L23) |
| 0x10024 | native::IPC_SEND | IPC / 0.8.0 / active | canal, *const SendV1, 160 → octets acceptés | SEND | BAD_VERSION, BAD_ARGUMENT, BAD_ADDRESS, DENIED, LIMIT, WOULD_BLOCK, TIMED_OUT, CANCELLED, REVOKED, CLOSED, NO_MEMORY | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | [Endpoint::send](../libs/nova-system/src/ipc.rs#L47) |
| 0x10025 | native::IPC_RECEIVE | IPC / 0.8.0 / active | canal, *mut ReceiveV1, 184 → type du message | RECEIVE | BAD_VERSION, BAD_ARGUMENT, BAD_ADDRESS, DENIED, LIMIT, WOULD_BLOCK, TIMED_OUT, CANCELLED, REVOKED, CLOSED, NO_MEMORY | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | [Endpoint::receive](../libs/nova-system/src/ipc.rs#L50) |
| 0x10026 | native::IPC_GIVE | IPC / 0.8.0 / active | canal, *const GiveV1, 48 → 0 | SEND et GRANT, droits de la source | BAD_VERSION, BAD_ARGUMENT, BAD_ADDRESS, DENIED, LIMIT, TIMED_OUT, CANCELLED, REVOKED, CLOSED, REFUSED, NO_MEMORY | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | [Endpoint::give](../libs/nova-system/src/ipc.rs#L28) |
| 0x10027 | native::IPC_NOTIFY | IPC / 0.8.0 / active | canal, bits non nuls, — → 0 | SEND | DENIED, LIMIT, BAD_ARGUMENT, CANCELLED | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | — |
| 0x10028 | native::IPC_CANCEL | IPC / 0.8.0 / active | handle, —, — → attentes annulées | MANAGE | DENIED, CANCELLED | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | — |
| 0x10029 | native::IPC_BOOTSTRAP | IPC / 0.8.0 / active | — → handle du lanceur | alias local installé | DENIED, CANCELLED | [start_call](../kernel/src/sched/process/ipc/traffic.rs#L345) | — |
| 0x1002c | native::IPC_PUT | IPC / 0.16.6 / active | capacité, *const BulkV1, 32 → octets transférés | SEND et canal page lié | DENIED, BAD_ARGUMENT, BAD_ADDRESS, WOULD_BLOCK | [transfer](../kernel/src/sched/process/ipc/bulk.rs#L67) | — |
| 0x1002d | native::IPC_TAKE | IPC / 0.16.6 / active | capacité, *const BulkV1, 32 → octets transférés | RECEIVE et canal page lié | DENIED, BAD_ARGUMENT, BAD_ADDRESS, REFUSED | [transfer](../kernel/src/sched/process/ipc/bulk.rs#L67) | — |
| 0x1003b | native::IPC_FEATURES | IPC / 0.16.6 / active | capacité, direction, 1 → masque de fonctions | SEND ou RECEIVE selon la direction | DENIED, BAD_ARGUMENT | [features](../kernel/src/sched/process/ipc/bulk.rs#L46) | — |
| 0x10040 | block::PORTAL | pilote bloc / 0.10.0 / active | bail, *mut trame bloc, 128 → 128 | pilote titulaire, thread principal | DENIED, BAD_ARGUMENT, BAD_ADDRESS, REVOKED, CLOSED, REFUSED | [device](../kernel/src/sched/process/syscall.rs#L233) | — |
| 0x10041 | native::HEAP_RESIZE | mémoire / 0.11.0 / active | ancienne borne, nouvelle borne, 0 → borne exclusive du tas | tas du processus, zéro/zéro consulte la borne | BAD_ARGUMENT, NO_MEMORY | [call](../kernel/src/sched/process/syscall.rs#L99) | [memory::heap_end](../libs/nova-system/src/lib.rs#L67), [memory::resize_heap](../libs/nova-system/src/lib.rs#L76) |
| 0x10042 | terminal::SPAWN | lanceur / 0.13.0 / active | indice, *const paquet, longueur → droit de suivi | LAUNCH, thread principal et catalogue | DENIED, BAD_ARGUMENT, BAD_ADDRESS, LIMIT, NO_MEMORY | [spawn_program](../kernel/src/sched/process/ipc/spawn.rs#L71) | — |
| 0x10043 | terminal::CONSOLE | terminal / 0.13.0 / active | opération, adresse ou sous-opération, taille ou arguments → selon le port | droits terminal de la session, thread principal | DENIED, BAD_ARGUMENT, BAD_ADDRESS, TIMED_OUT, REFUSED | [console_io](../kernel/src/sched/process/syscall.rs#L385) | — |
| 0x10044 | native::CLOCK_UTC | horloge / 0.14.0 / active | 0, 0, 0 → secondes UTC depuis 1970 | aucun droit additionnel | REFUSED, BAD_ARGUMENT | [call](../kernel/src/sched/process/syscall.rs#L99) | [clock::utc_seconds](../libs/nova-system/src/lib.rs#L61) |
| 0x10045 | terminal::PROCESS_SAMPLE | observation / 0.16.3 / active | indice, *mut sortie, 24 → 0 absent ou 1 présent | OBSERVE_SESSION, thread principal | DENIED, BAD_ARGUMENT, BAD_ADDRESS | [sample_process](../kernel/src/sched/process/syscall.rs#L341) | — |
| 0x10046 | block::PAGE_PORTAL | pilote bloc / 0.16.6 / active | bail, *mut trame bloc, adresse page → 4096 | pilote titulaire, thread principal | DENIED, BAD_ARGUMENT, BAD_ADDRESS, REVOKED, CLOSED, REFUSED | [device_page](../kernel/src/sched/process/syscall.rs#L193) | — |
| 0x10060 | native::THREAD_CREATE | threads applicatifs / 0.20.0 / active | adresse RX, argument scalaire, pages pile → droit de jonction | extension admise au catalogue | DENIED, BAD_ARGUMENT, BAD_ADDRESS, LIMIT, NO_MEMORY | [create_member](../kernel/src/sched/process/threads.rs#L81) | [thread::spawn](../libs/nova-system/src/thread.rs#L18) |
| 0x10061 | native::THREAD_EXIT | threads applicatifs / 0.20.0 / active | résultat, 0, 0 → sans retour | extension admise | DENIED, BAD_ARGUMENT | [exit](../kernel/src/sched/process/threads.rs#L226) | [thread::exit](../libs/nova-system/src/thread.rs#L13) |
| 0x10062 | native::THREAD_JOIN | threads applicatifs / 0.20.0 / active | droit, 0, 0 → résultat du thread | droit de jonction du processus | DENIED, BAD_ARGUMENT, REFUSED, CANCELLED | [join](../kernel/src/sched/process/threads.rs#L259) | [JoinHandle::join](../libs/nova-system/src/thread.rs#L46) |
| 0x10063 | native::THREAD_DETACH | threads applicatifs / 0.20.0 / active | droit, 0, 0 → 0 | droit de jonction du processus | DENIED, REFUSED, BAD_ARGUMENT | [detach_member](../kernel/src/sched/process/threads.rs#L295) | [JoinHandle::detach](../libs/nova-system/src/thread.rs#L52) |
| 0x10064 | native::THREAD_CANCEL | threads applicatifs / 0.20.0 / active | droit, 0, 0 → 0 | droit local sur la génération visée | DENIED, CLOSED, BAD_ARGUMENT | [cancel](../kernel/src/sched/process/threads.rs#L307) | [JoinHandle::cancel](../libs/nova-system/src/thread.rs#L41) |
| 0x10065 | native::THREAD_CANCELLED | threads applicatifs / 0.20.0 / active | 0, 0, 0 → 0 ou 1 | extension admise | DENIED, BAD_ARGUMENT | [call](../kernel/src/sched/process/threads.rs#L416) | [thread::cancelled](../libs/nova-system/src/thread.rs#L73) |
| 0x10066 | native::THREAD_TLS | threads applicatifs / 0.20.0 / active | 0, 0, 0 → adresse de la page TLS | extension admise | DENIED, BAD_ARGUMENT, NO_MEMORY | [tls](../kernel/src/sched/process/threads.rs#L402) | [thread::tls_get](../libs/nova-system/src/thread.rs#L84), [thread::tls_set](../libs/nova-system/src/thread.rs#L88) |
| 0x10067 | native::THREAD_WAIT | threads applicatifs / 0.20.0 / active | *mut mot aligné, valeur attendue, délai ticks → 0 | extension admise, mot RW local | DENIED, BAD_ARGUMENT, BAD_ADDRESS, WOULD_BLOCK, TIMED_OUT, CANCELLED, LIMIT | [wait_word](../kernel/src/sched/process/threads.rs#L338) | [sync::wait](../libs/nova-system/src/sync.rs#L10) |
| 0x10068 | native::THREAD_NOTIFY | threads applicatifs / 0.20.0 / active | *mut mot aligné, maximum, 0 → nombre de réveils | extension admise, mot RW local | DENIED, BAD_ARGUMENT, BAD_ADDRESS | [notify](../kernel/src/sched/process/threads.rs#L372) | [sync::notify](../libs/nova-system/src/sync.rs#L19) |
| 0x10070 | instances::INFO | instances / 0.21.0 phase 1 / active | mode, sortie éventuelle, 0 ou 32 → numéro local, installation ou 32 | instance de l’appelant | BAD_ARGUMENT, REFUSED, BAD_ADDRESS, CANCELLED | [instance_call](../kernel/src/sched/process/ipc/spawn.rs#L197) | — |
| 0x10071 | instances::WORKER | instances / 0.21.0 phase 1 / active | *const identité, argument, 32 → droit de suivi | principal admis, même instance, thread principal | DENIED, BAD_ARGUMENT, BAD_ADDRESS, REFUSED, LIMIT, NO_MEMORY, CANCELLED | [instance_call](../kernel/src/sched/process/ipc/spawn.rs#L197) | — |
| 0x10072 | instances::STOP | instances / 0.21.0 phase 1 / active | droit de suivi, *const identité, 32 → 0 | offre de la cible et droit de suivi, thread principal | DENIED, BAD_ARGUMENT, BAD_ADDRESS, REFUSED, CANCELLED | [instance_call](../kernel/src/sched/process/ipc/spawn.rs#L197) | — |
| 0x10073 | instances::OFFER_STOP | instances / 0.21.0 phase 1 / active | *const identité, 0, 32 → 0 | principal admissible et identité propre | DENIED, BAD_ARGUMENT, BAD_ADDRESS, REFUSED, CANCELLED | [instance_call](../kernel/src/sched/process/ipc/spawn.rs#L197) | — |
| 0x10074 | instances::WATCH | instances / 0.21.0 phase 1 / active | étape, incarnation volatile, 0 → selon étape | gestionnaire lié | DENIED, BAD_ARGUMENT, REFUSED | [watch](../kernel/src/sched/process/supervision.rs#L77) | — |
| 0x10075 | instances::OFFERED | instances / 0.21.0 phase 1 / active | droit de suivi, *mut identité, 32 → 32 | offre de la cible et droit de suivi, thread principal | DENIED, BAD_ARGUMENT, BAD_ADDRESS, REFUSED, CANCELLED | [instance_call](../kernel/src/sched/process/ipc/spawn.rs#L197) | — |

Les pointeurs sont des adresses virtuelles de l’appelant, transmises sur 64 bits. Sur x86_64, SYSCALL détruit RCX et R11. Sur AArch64, x2 à x30 et SP_EL0 sont préservés. [Contrat des registres](abi.md).

Plages préservées par le registre : 0x1002a…0x1002b, 0x1002e…0x1003a. [Historique des sondes](es-v0.16.6-integration-interfaces.md).

## Protocoles en espace utilisateur

Le numéro de message est local à son protocole. Le service fichier NovaFS traite les requêtes du SDK via IPC. Le pilote bloc utilise son propre format. Le canal d’édition relie vi à nsh. L’admission des instances utilise une page privée et la porte noyau WATCH. [Gestionnaire](../user/instance_manager.rs#L40).

| Protocole | Opcode | Opération | Trame | Traitement ou transport | SDK Rust |
|---|---|---|---|---|---|
| fichiers | 1 | MOUNT | [files.rs](../abi/files.rs) | [dispatch](../user/novafs/files.rs#L141) | — |
| fichiers | 2 | INFO | [files.rs](../abi/files.rs) | [dispatch](../user/novafs/files.rs#L141) | — |
| fichiers | 3 | OPEN | [files.rs](../abi/files.rs) | [dispatch](../user/novafs/files.rs#L141) | — |
| fichiers | 4 | READ | [files.rs](../abi/files.rs) | [dispatch](../user/novafs/files.rs#L141) | [File::read](../libs/nova-system/src/files.rs#L62), [Session::read_view](../libs/nova-system/src/files.rs#L23) |
| fichiers | 5 | WRITE | [files.rs](../abi/files.rs) | [dispatch](../user/novafs/files.rs#L141) | [File::write](../libs/nova-system/src/files.rs#L67) |
| fichiers | 6 | SYNC | [files.rs](../abi/files.rs) | [dispatch](../user/novafs/files.rs#L141) | [File::sync](../libs/nova-system/src/files.rs#L72) |
| fichiers | 7 | CLOSE | [files.rs](../abi/files.rs) | [dispatch](../user/novafs/files.rs#L141) | [File::close](../libs/nova-system/src/files.rs#L75) |
| fichiers | 8 | PATH_FRAGMENT | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | — |
| fichiers | 9 | DIR_OPEN | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | [Session::directory](../libs/nova-system/src/files.rs#L48) |
| fichiers | 10 | LOOKUP_AT | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | — |
| fichiers | 11 | OPEN_AT | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | [Session::open](../libs/nova-system/src/files.rs#L40) |
| fichiers | 12 | READDIR | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | [Directory::read_fragment](../libs/nova-system/src/files.rs#L94), [Session::read_directory_view](../libs/nova-system/src/files.rs#L28) |
| fichiers | 13 | MKDIR_AT | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | — |
| fichiers | 14 | RMDIR | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | — |
| fichiers | 15 | UNLINK_AT | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | — |
| fichiers | 16 | RENAME_AT | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | — |
| fichiers | 17 | STAT | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | — |
| fichiers | 18 | DIR_CLOSE | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | [Directory::close](../libs/nova-system/src/files.rs#L104) |
| fichiers | 19 | CHDIR | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | — |
| fichiers | 20 | GETCWD | [files.rs](../abi/files.rs) | [directory_request](../user/novafs/files/namespace.rs#L105) | — |
| fichiers | 21 | DELEGATE | [files.rs](../abi/files.rs) | [serve](../user/novafs/files.rs#L628) | — |
| fichiers | 22 | STAGE | [files.rs](../abi/files.rs) | [serve](../user/novafs/files.rs#L628) | — |
| fichiers | 23 | CREATE_NEW | [files.rs](../abi/files.rs) | [execute](../user/novafs/files/selected.rs#L187) | — |
| fichiers | 24 | MOVE_SELECTED | [files.rs](../abi/files.rs) | [execute](../user/novafs/files/selected.rs#L187) | — |
| fichiers | 25 | TOUCH_SELECTED | [files.rs](../abi/files.rs) | [execute](../user/novafs/files/selected.rs#L187) | — |
| fichiers | 26 | PUBLISH_COPY | [files.rs](../abi/files.rs) | [execute](../user/novafs/files/selected.rs#L187) | — |
| fichiers | 27 | COPY_RANGE | [files.rs](../abi/files.rs) | [execute](../user/novafs/files/selected.rs#L187) | — |
| fichiers | 28 | READ_PAGE | [files.rs](../abi/files.rs) | [read_page](../user/novafs/files.rs#L582) | — |
| bloc | 1 | INFO | [block.rs](../abi/block.rs) | [driver](../user/storage/disk.rs#L11) | — |
| bloc | 2 | READ | [block.rs](../abi/block.rs) | [driver](../user/storage/disk.rs#L11) | — |
| bloc | 3 | WRITE | [block.rs](../abi/block.rs) | [driver](../user/storage/disk.rs#L11) | — |
| bloc | 4 | FLUSH | [block.rs](../abi/block.rs) | [driver](../user/storage/disk.rs#L11) | — |
| bloc | 5 | CLOSE | [block.rs](../abi/block.rs) | [driver](../user/storage/disk.rs#L11) | — |
| bloc | 7 | DISCOVER | [block.rs](../abi/block.rs) | [driver](../user/storage/disk.rs#L11) | — |
| edition | 1 | INFO | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 2 | READ | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 3 | KEY | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 4 | BEGIN | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 5 | NAME | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 6 | TEXT | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 7 | SAVE | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 8 | FIN | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 9 | VIEW | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 10 | LOCATE | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 11 | CLEAR_TAIL | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |
| edition | 12 | VERIFY | [vi_wire.rs](../../components/libraries/vi_wire.rs) | [reply](../../components/shell/vi.rs#L290) | — |

## Utiliser le registre

Depuis la racine du dépôt, exécuter `make -C novaOS abi-generate` après édition de [syscalls.toml](../abi/syscalls.toml). La cible `make -C novaOS abi-check` compare les produits sans les écrire. Le noyau, la bibliothèque utilisateur et nova-system importent les constantes produites. L’en-tête [C11 / C++11](../abi/generated/nova_abi.h) expose constantes et structures. Le SDK Rust autonome livre le fichier native.rs généré avec la présentation de v0.2.0 pour conserver son empreinte.

Les structures et les statuts figurent dans [le contrat de disposition](../abi/native.rs.in). Les 32 octets d’une identité durable sont décrits par [Identity::encode](../abi/instances.rs#L27).
