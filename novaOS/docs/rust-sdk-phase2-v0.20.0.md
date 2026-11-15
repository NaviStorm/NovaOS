# NovaOS v0.20.0 · phase 2 du SDK Rust

La branche `codex/rust-system-0200-phase2` part de la fusion de phase 1 `90de918fa6ca090d731500127cf400c4836a0fec`. La bibliothèque `nova-system` passe de 0.1.0 à 0.2.0 pour identifier le changement de son contrat source. Le numéro de construction de NovaOS reste 0.20.0. La fusion et la clôture de la version sont réservées à la MOA/MOE. [Qualification précédente](qualification-rust-threads-v0.20.0.md)

## Propriété du droit de jonction

En 0.1.0, `join(self)` prenait le handle par valeur. Si le noyau interrompait la jonction, l’opérateur `?` faisait sortir de la méthode et `Drop` tentait de détacher la cible. Pourtant, le noyau venait de libérer la place du collecteur et conservait le droit de jonction. Un client pouvait ainsi perdre le résultat précisément lorsqu’il devait décider qui reprendrait l’attente.

En 0.2.0, `join(&self)` et `detach(&self)` empruntent le handle. L’objet Rust garde un propriétaire. Une jonction réussie consomme le droit natif, rapporte le résultat une fois et marque le handle clos. Un détachement réussi le marque également clos. Une erreur laisse la décision au propriétaire. Une demande d’annulation reste attachée au thread annulé. Celui-ci peut déplacer le handle vers un autre membre du même processus, via un objet partagé sous mutex. Pour pouvoir réessayer, le client doit conserver le handle dans une variable ou un conteneur. Dans une expression comme `spawn::<T>(arg, 4)?.join()`, le temporaire atteint sa fin de vie en fin d’expression. La destruction du dernier objet propriétaire tente le détachement du droit restant. [Implémentation Rust](../libs/nova-system/src/thread.rs)

| Situation | Comportement observable |
|---|---|
| B est vivant, A possède son droit | `join()` endort A. Un autre collecteur ne peut prendre la même attente. |
| A est annulé alors qu’il attend B | A reçoit `CANCELLED`, reste propriétaire et peut transmettre le handle. L’annulation coopérative d’A persiste. |
| C reçoit le handle par déplacement | C peut attendre B, même si B a déjà terminé. La première collecte réussie ferme le droit. |
| Deuxième collecte ou collecte après détachement réussi | Le SDK rend `DENIED` sur le handle clos. Le noyau refuse également une ancienne génération. |
| Auto-jonction dans l’ABI native | Refus `BAD_ARGUMENT`. La destruction du processus clôt les droits et permet la restitution des ressources. |
| Échéance | `THREAD_JOIN(droit, 0, 0)` a deux arguments réservés à zéro. Le délai en ticks appartient à `THREAD_WAIT`, l’attente sur mot atomique. |

Le changement concerne la propriété Rust. Les binaires déjà compilés avec le SDK 0.1.0 gardent le comportement de leur wrapper jusqu’à leur recompilation. Un ancien noyau muni de l’extension de phase 1 sait exécuter la jonction du nouveau wrapper. La mémoire est partagée entre membres d’un processus. Les piles, les pages de garde, les générations, le TLS et les huit places secondaires gardent le contrat de phase 1. [Contrat natif](rust-threads-v0.20.0.md#mémoire-collecte-et-isolation)

## Qualification du 15 novembre 2026

| Essai | AArch64 | x86_64 |
|---|---|---|
| Exemple public du SDK, 2 vCPU, 256 Mio, QEMU 11.1.3 TCG | ELF de 28 400 octets exécuté en EL0, sortie 0 et restitution vérifiée | ELF de 22 080 octets exécuté en Ring 3, sortie 0 et restitution vérifiée |
| Sonde des threads du SDK, même géométrie | 62 490 cadres libres avant et après, 6 et 6 préemptions, deux migrations | 63 017 cadres libres avant et après, 5 et 5 préemptions, deux migrations |
| Interruption de A et transfert à C | C collecte le résultat 63 de B, deuxième collecte refusée | Même résultat |
| Tests unitaires de la crate extraite | 3/3 sur Darwin ARM64 | 3/3 sous Rosetta |
| Contrôle de types sur cible nue | `JoinHandle: Send` accepté, `Copy` et `Clone` refusés | Mêmes résultats |
| Matrice du SDK 0.1.0 et 0.2.0, à 1 vCPU | 4 exécutions de clients et 2 lectures par le `cat` livré | 4 exécutions de clients et 2 lectures par le `cat` livré |

Le contrôleur observe A bloqué sur la jonction de B et B endormi sur un mot, avant d’autoriser l’annulation de A. Le test compare la génération du handle avant et après le retour `CANCELLED`. A déplace le handle dans une boîte sous mutex. Une fois A terminé, C le prend par déplacement, réveille B en demandant son annulation coopérative, collecte 63 puis reçoit `DENIED` à la deuxième tentative. Le banc vérifie que les trois identités sont distinctes, que la boîte est vide et que la génération précédente est refusée après réemploi du slot. L’expiration de l’attente sur mot est vérifiée avec un délai d’un tick. [Sonde utilisateur](../tests/external-threads/checks.rs) · [Contrôleur invité](../kernel/src/sched/process/ipc/thread_probe.rs)

Les sondes de chaque ISA vérifient aussi la somme 432 sous mutex, les valeurs TLS 7 et 11, FPU/SIMD après préemption et migration, le défaut d’accès à la garde de pile, le nettoyage des pages, la révocation IPC et le refus d’un appel sans droit. La destruction avec cinq membres en calcul ou en attente laisse zéro survivant. Les campagnes à 1, 2 et 4 vCPU de phase 1 apportent les autres géométries pour le noyau de production inchangé. Les essais ciblés de cette phase portent sur QEMU et sur les deux environnements Darwin. Les campagnes longues de stockage, UTM et le matériel physique sont hors de ce verdict. [Mesures et empreintes par invité](validation/rust-sdk-phase2-v0.20.0.json) · [Matrice ancien/nouveau et reproduction](compat-rust-sdk-v0.20.0.md)

### SDK vérifié hors dépôt

L’archive finale contient 23 fichiers et pèse 21 237 octets. Son SHA-256 est `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`. Les quatre premières exécutions QEMU ont utilisé une archive dont le guide nécessitait de préciser le chemin de `rustdoc`. Sur le Mac d’essai, Cargo trouvait un `rustdoc` d’une autre chaîne, dépourvu de la cible nue. Le guide précise désormais la sélection conjointe de Rust et rustdoc par rustup. Le guide est le seul des 23 membres modifié. Une reconstruction depuis l’archive finale, aux mêmes chemins, a produit quatre ELF identiques octet par octet aux entrées des catalogues effectivement exécutées.

Un second projet Cargo placé hors du dépôt et hors de l’extraction, dans un chemin contenant des espaces, a également compilé hors ligne sur les deux cibles. Ses ELF mesurent 28 440 octets sur AArch64 et 22 120 sur x86_64. Ce second contrôle porte sur la compilation. La génération locale de la documentation de la crate a réussi avec la recette corrigée. [Guide du SDK et exemples](sdk-rust-v0.20.0.md) · [Archive](../../distribution/sdk/nova-system-0.2.0.tar.gz)

Commandes du banc à exécuter depuis `novaOS/` avec la chaîne Rust et les cibles nues installées :

```sh
for isa in aarch64 x86_64
do
    NOVA_ARCH="$isa" python3 -B tests/threads.py --vcpus 2 --exemple-sdk
    NOVA_ARCH="$isa" python3 -B tests/threads.py --vcpus 2
done
```

**Décision proposée.** Soumettre cette phase à la fusion après validation MOA/MOE. Les preuves couvrent le SDK croisé `no_std`, l’admission explicite d’un ELF et la reprise de jonction par transfert. La MOA/MOE doit encore décider de la surface publique à figer et des artefacts exigés pour clore v0.20.0. Le TLS propre au compilateur, `std::thread`, l’admission générale d’un fichier et la compilation dans NovaOS restent hors du périmètre qualifié. La compilation native Rust/C/C++ demeure une exigence de v0.30.0. [Décisions de la feuille de route](reconciliation-v0300/decisions.md)

## Nettoyage Git du 15 novembre 2026

L’inventaire a comparé chaque HEAD à `main` et à sa référence distante, consulté l’état des fichiers suivis, non suivis et ignorés, les chats Codex actifs et les fichiers ouverts des worktrees candidats au retrait. Les deux worktrees retirés étaient propres, intégrés dans `main`, sans fichiers ignorés et sans tâche active. Chaque retrait a utilisé `git worktree remove` puis `git branch -d`. Leurs branches distantes ont été conservées.

| Worktree ou branche locale | Décision | Motif |
|---|---|---|
| `codex/nvme-conception-0190` · `d6a84a35c15d` | Retiré | Conception intégrée et publiée. |
| `codex/roadmap-reconciliation-v0300` · `cfcf3ab64d7d` | Retiré | Feuille de route intégrée et publiée. |
| `codex/audit-v0300` · `a602051c9f3f` | Conservé | Cette branche distante et locale n’est pas fusionnée dans `main`. |
| `codex/finalisation-smp-0170` · `573137c1f567` | Conservé | Relevés de performance et journaux de récupération après coupure hors Git. |
| `codex/materiel-0180` · `a467fe686758` | Conservé | Traces matérielles, PCI et IPC hors Git. Les trois commits en avance sur la branche distante sont dans `main`. |
| `codex/nvme-0190` · `8839cff384e6` | Conservé | Journaux série NVMe/IPC et autres artefacts de qualification locaux. |
| `codex/rust-system-0200` · `6533078fc0fe` | Conservé | Journaux et exécutables originaux des preuves Rust de phase 1. |
| `main` et `codex/rust-system-0200-phase2` | Conservés | Copie de travail principale de Thierry et branche active du mandat. |

Les deux entrées non suivies `linux`, `linux_MacBookThierry.local_Nov-14-125114-2026_Conflict` et la modification locale de `.gitignore` appartiennent à la copie principale. Les branches locales sans worktree ne faisaient pas l’objet d’un retrait.
