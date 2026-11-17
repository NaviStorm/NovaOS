# CUT — qualification QEMU du 17 novembre 2026

**Quatre profils réussis sur quatre.** Chaque invité a exécuté 1 954 vecteurs du parseur et du moteur, puis a rendu le statut 0 après l’ensemble des contrôles. La sonde exerce aussi trois scénarios de champs longs avant la matrice. Au total, 1 957 contrôles fonctionnels par profil donnent 7 828 exécutions pour les quatre démarrages.

| Invité | Secteur P9 | TSV `-f1,3,8,9` | Complément de champs | Sélection de caractères UTF-8 | Résultat de l’ELF sonde |
|---|---:|---:|---:|---:|---|
| AArch64 | 512 | 7,8416 s | 0,5728 s | 0,8357 s | EL0, sortie 0, restitution OK |
| AArch64 | 4 096 | 7,8984 s | 0,5638 s | 0,8050 s | EL0, sortie 0, restitution OK |
| x86_64 | 512 | 8,2609 s | 0,6336 s | 0,8472 s | Ring 3, sortie 0, restitution OK |
| x86_64 | 4 096 | 8,0922 s | 0,6191 s | 0,8550 s | Ring 3, sortie 0, restitution OK |

Les durées sont les médianes des trois passages après échauffement. Les trois traitements exercent respectivement 285 214 720, 16 780 800 et 16 779 776 octets. Chaque noyau voit deux vCPU et 268 435 456 octets de RAM ont été mesurés par QMP. Le flux TSV de 272,002 Mio dépasse les 256 Mio de RAM des quatre invités. Chaque traitement contrôle une sortie attendue avant sa boucle de mesure. Les 12 passages par profil, soit 48 au total, sont conservés individuellement. Ils mesurent le moteur sur des données répétées en mémoire, avec un P9 neuf attaché en lecture seule. Les comparaisons au plafond 1,10 × GNU concernent la campagne Darwin. [Résultats Darwin](../creneau/confirmation.txt)

## Identité des exécutables

Sources CUT et sonde : `f2a0485388679d44e53b19f4455b96aa3a516135`. Livraison candidate utilisée pour les quatre invités : `8394c27c5f3909e0945ed65351a053d0d72ffd1f`. Le banc extrait les ELF du prévol de cette livraison et compare leurs octets au manifeste avant chaque démarrage. Il vérifie l’inclusion de la sonde dans le noyau, puis l’identité du noyau avec celui extrait de l’ISO donnée à QEMU. La matrice de 1 954 vecteurs est régénérée et son SHA-256 correspond au prévol : `93a62f7b2168e721bcfde5824bd050f4efdc35f00599d97cfab400b8dab4611c`.

| Architecture | SHA-256 sonde exécutée | SHA-256 ELF CUT de commande contrôlé |
|---|---|---|
| AArch64 | `448c7c1529fe1f0102fd6718a4cdac2adc31e586e0111bb2220fe3ff0665c662` | `bb5d5b38b0e5698b5fb78a894948ed02121210e2b6d0a9c8db80683d53a0eec6` |
| x86_64 | `804b3b953e9654e5720f1f8a8a6daf8ef94ed8db3d119f4bbd98416d929fc591` | `509a7d546db407ee07571e4d071957ea39efe617746205282a1b94ceb4d3fb1e` |

La sonde SDK compile directement les modules Rust du candidat. Elle compare les octets avant rendu et les erreurs du parseur à l’oracle GNU. Les trois scénarios `-f1,3`, `-sf1` et `-sf2` contrôlent des blocs de 1 Kio traversant un champ de 4 Kio, un TAB tardif et une dernière ligne sans TAB ni LF. Ils précèdent le marqueur de réussite de la matrice dans le code exact de la sonde. Après la sortie du processus, le noyau contrôle les slots et la restitution des ressources. Les journaux série conservent le PID, le niveau de privilège, la sortie 0 et le marqueur de restitution.

L’ELF de commande figure dans les artefacts contrôlés. Son exécution métier par nom, par chemin ou dans `cat donnees.tsv | cut -f1,3` depuis le vrai `nsh` reste à qualifier dans le chantier du lanceur commun. Les traces historiques du vrai shell restent celles de la livraison `008d62b`. [Code de la sonde](../../lab/sonde.rs) · [Témoins historiques](../qemu)

## Preuves et relecture

Les quatre volumes jetables ont gardé la même empreinte avant et après QEMU. La relecture indépendante par NovaFS a vérifié intégralement chaque volume. Les 28 membres de l’archive ont été relus et comparés à leur SHA-256. Les copies identiques partagent leur contenu dans le tar. Son empreinte est `ebf930198953f5fa2bc85afa35bae5fdd6b09b57c6eb02af20a77831fe806b36`. Les 117 entrées du manifeste de départ à `8394c27` ont été vérifiées avant la campagne. [Médias](medias.tar.gz) · [Manifeste des médias](manifeste.json) · [Vérification par profil](verification.txt)

Pour relire les preuves existantes depuis la racine du worktree :

```sh
python3 -B livraison-cut-0240/preuves/qemu-candidat/verifier.py
(cd livraison-cut-0240 && shasum -a 256 -c SHA256SUMS)
```

Pour refaire les quatre essais avec un espace de travail isolé et les mêmes ELF :

```sh
export NOVAOS_TMPDIR="$(cd ../tmp && pwd)"
export TMPDIR="$NOVAOS_TMPDIR/"
(cd novaFS && CARGO_TARGET_DIR="$PWD/target" rustup run stable cargo build --release --locked --offline --target aarch64-apple-darwin --bin novafs)
campagne=$(mktemp -d "$NOVAOS_TMPDIR/cut-qemu-8394c27.XXXXXXXX")
export NOVAOS_TMPDIR="$campagne" TMPDIR="$campagne/"
for arch in aarch64 x86_64; do
  for secteur in 512 4096; do
    python3 -B livraison-cut-0240/lab/sonde.py "$arch" "$secteur" "$campagne/$arch-$secteur" --large --prevol || exit 1
  done
done
```

La branche locale est `codex/cut-bancs-v0300`. La cible de publication à autoriser est `origin/codex/cut-bancs-v0300`. Elle réunit les bancs `fb7d050`, les correctifs et contrôles `f1767ef` et `f2a0485`, la qualification Darwin `8394c27` et la qualification QEMU de ce rapport. La référence distante historique `origin/codex/cut-natif-0240` a été vérifiée à `008d62b82d5d7c7ffb601bb15a0bdee09af1b6c1`.
