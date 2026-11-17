# Installation signée et courtier de session en invité QEMU

Le commit `ac87899a4aa57118e370cc50843cddca6977448c` exerce le lanceur dans le profil de qualification NovaAuth. Le signataire de session, l’autorité de distribution et le provisionneur ont des fonctions distinctes. Le courtier peut remettre un ticket ou une extrémité déjà autorisée. Il ne reçoit aucun accès aux fichiers utilisateur ni aucune clé privée.

| Épreuve | AArch64 P9/512 | AArch64 P9/4096 | x86_64 P9/512 | x86_64 P9/4096 |
|---|---:|---:|---:|---:|
| Flux et tickets, parcours nominal et deux points de panne pour chacun | 6/6 | 6/6 | 6/6 | 6/6 |
| Interruption de publication avant/après décision, puis relecture du même disque et de la même autorité | 2/2 | 2/2 | 2/2 | 2/2 |

Sur les quatre profils, NovaAuth et le courtier utilisent chacun quatre racines. Le cinquième canal et sa création indirecte sont refusés. Les offres concurrentes, les tickets expirés ou révoqués et la panne du courtier sont consignés dans les traces. Le parcours de ticket lance le véritable ELF signé de `cut` sans descripteurs. Son refus d’accès (statut 5) est attendu. Le shell retrouve le PID après abandon de la première réponse et collecte une seule fois l’enfant. Cette épreuve qualifie la remise du ticket. Les invocations métier avec flux et fichiers exigent des essais distincts.

Les traces comprennent les essais de mise au point. Un appel IPC déjà suspendu était intercepté une deuxième fois au réveil. Le traitement laisse désormais l’attente enregistrée se terminer avant une nouvelle interception. Un essai de courte échéance a expiré avant remise au shell. La recette d’expiration mesure désormais l’attente après remise.

Les neuf contrôles ABI v1 réussissent sur la source ordinaire et la copie de qualification. Le registre contient 48 sélecteurs. Les quatre ELF historiques sont tracés dans les archives. Les noyaux ordinaires ont conservé les empreintes SHA-256 `17ab08f45b11cd5bb089a2d384a2df0c95e5c123e013f15dc475d85a44d395e2` et `32bad229fee23fc3c9e4f74b2ae20518f414a2e38934d6c71102138b4e6b94ca` (association architecture/empreinte dans `ordinaires-courtier.json`).

## Preuves

Le manifeste Git `validation/lancement-generique-courtier.sha256` couvre 706 fichiers. L’archive privée `OS-worktrees/archives-lancement-generique-elf/courtier-et-installation-ac87899.tar.gz` a été relue par décompression et comparaison SHA-256 de chaque membre. Son SHA-256 est `7eb501f31cdcfaca4c3d6b8a6a0a5918f73a8676897cf0a8c8e393d2ef2ed228`, celui du manifeste est `f470a79ddeed340fda8d61a40f097bf9814304e7767517080f5ab6a2e7458343`. Les images de qualification restent dans cette archive privée. Les arrêts QEMU et l’autorité logicielle de laboratoire ne qualifient ni une coupure électrique physique ni l’autorité de production.
