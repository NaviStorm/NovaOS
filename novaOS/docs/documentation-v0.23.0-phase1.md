# Documentation native de NovaOS v0.23.0, phase 1

## Contrat et provenance

Le catalogue embarqué fournit l’aide et le manuel en **français ou en anglais**. Les commandes, options, identifiants et valeurs du protocole conservent leur orthographe native. Depuis l’invite, essayer `man man`, `service stop --help`, `cd --version` ou `man service capabilities restrict`.

Les huit entrées Rust associent le nom et le composant à la description, au synopsis, aux arguments et options, aux sous-commandes, aux exemples, aux droits, aux effets, aux codes, aux erreurs et aux limites. Chaque champ traduisible regroupe les deux textes. Une même entrée produit l’aide courte, le manuel et les pages imbriquées. Le numéro provient de `CARGO_PKG_VERSION` du paquet qui construit les commandes. La version des pilotes est 0.20.0. Le jalon des sources en développement est v0.23.0. [Catalogue](../../components/libraries/documentation.rs) · [Manifeste](../../components/Cargo.toml)

| Entrées | Exécution habituelle | Fournisseur affiché |
|---|---|---|
| `nsh` | Shell et interprétation de scripts | nsh 0.20.0 |
| `cd`, `journal`, `service`, `man` | Commandes internes | Intégrée à nsh, nsh 0.20.0 |
| `ls`, `rm`, `vi` | Exécutables natifs | nova-cli 0.20.0 |

Après l’analyse des mots et des alias, le shell traite les demandes documentaires avant le globbing, la résolution des chemins, le choix d’une délégation ou l’envoi de commandes aux services. Pour les trois utilitaires, il emploie le parseur d’options de l’ELF. Les exécutables conservent leur entrée d’information directe. Les observations dans les invités portent sur l’invocation publique depuis `nsh`. [Répartiteur](../../components/shell/script.rs) · [Options](../../components/shell/doc_options.rs)

`man` utilise les primitives texte natives du terminal. Il mesure la géométrie par une réponse du curseur et utilise 80 × 24 en son absence. Le découpage visuel respecte la largeur des caractères Unicode. Les flèches et j/k déplacent d’une ligne, espace/b d’une page, g/G jusqu’aux extrémités. La touche / ouvre une recherche littérale sensible à la casse, limitée à 96 octets. Entrée valide, n passe à l’occurrence suivante et q rend le terminal au shell. Une page absente produit un diagnostic et le statut 20 avant l’entrée en plein écran. La recherche et le rendu travaillent sur une page à la fois. De futures pages peuvent réutiliser ce catalogue et ce lecteur. [Lecteur](../../components/shell/manual.rs) · [Entrée et écran](../../components/shell/screen_input.rs)

## Configurer la langue

La session utilise sa variable native `LANG`. Par exemple, saisir `export LANG=fr-CA` puis `service stop --help`, `rm --help` et `man rm`. L’export transmet la préférence aux appels imbriqués de nsh. `unset LANG` rétablit l’anglais. Chaque session possède ses variables. Les commandes ne consultent ni l’horloge, ni le fuseau horaire, ni le clavier.

La sous-étiquette principale doit être exactement `fr` (casse indifférente) et l’étiquette entière doit avoir la syntaxe BCP 47. Une seule traduction française sert toutes les régions. Une langue différente ou absente, une valeur vide, des séparateurs doublés, des sous-étiquettes mal placées ou des variantes et extensions répétées conduisent à l’anglais. Les formes POSIX comme `fr_CA.UTF-8` suivent aussi ce repli. Après correction, le sélecteur vérifie également l’enregistrement des sous-étiquettes d’écriture, de région et de variante dans un extrait IANA embarqué. Le programme fonctionne hors ligne. La recette initiale ci-dessous est conservée avec son périmètre. [Clôture corrigée et registre des anomalies](cloture-documentation-v0.23.0-phase1.md). [Sélecteur](../../components/libraries/documentation_locale.rs) · [RFC 5646, §2.1](https://www.rfc-editor.org/rfc/rfc5646#section-2.1)

### Consultation publique et exécution autonome

Le chemin public traite les options et consulte le catalogue dans la session. Un ELF autonome peut aussi recevoir le résultat du choix linguistique par ses arguments privés. Le lanceur encode l’option, le séparateur et la valeur normalisée `nsh-doc=fr` ou `nsh-doc=en`. L’omission du marqueur donne l’anglais. Aucun pointeur partagé, droit métier ou écriture d’une préférence globale n’intervient.

Le noyau réserve l’exemption de `tmp` aux images installées et immuables de ls, rm et vi, après l’admission habituelle. Il contrôle l’intégralité des arguments copiés, les deux canaux nuls, les seules sorties du terminal, le statut de préparation nul et l’absence de droit de créer des travailleurs. Le fils reçoit son propre espace mémoire et sa propre instance. Les opérandes supplémentaires, les entrées interactives et les délégations métier excluent ce chemin. Un lancement ordinaire conserve le contrat de son temporaire. Le format du message SPAWN et ses numéros sont ceux de l’ABI v1. [Admission](../kernel/src/sched/process/ipc/document_launch.rs) · [Lancement](../kernel/src/sched/process/ipc/spawn.rs)

Dans le seul profil de qualification, `@probe documentation rm --help` et `@probe documentation rm --version` lancent le véritable ELF et attendent son statut. La trace émise par le fils identifie son PID, la langue reçue et l’absence de canaux métier. Elle complète le test du chemin public dans la même session. [Banc des locales](../tests/documentation_locales.py)

## Recette du 16 novembre 2026

Base officielle `ce8f2d83792e4d83f6e4d7ffbce6400a288fbd27`. Invités QEMU, deux vCPU et 256 Mio. Chaque colonne concerne les sources de la branche expérimentée. Les résultats de navigation ont été contrôlés sur le flux du terminal de l’invité.

| Contrôle | AArch64 | x86_64 |
|---|---|---|
| P9/512, session ordinaire et session de maintenance | 71 observations de commandes | 71 observations de commandes |
| Huit aides et huit versions, sortie et code | 16 succès, code 0 | 16 succès, code 0 |
| Manuels des huit commandes et de la sous-commande de restriction | 9 ouvertures, retour 0 | 9 ouvertures, retour 0 |
| Aide locale de 14 verbes de service et de 10 verbes de journal | 24 succès | 24 succès |
| Option inconnue par pilote, page inexistante | Codes 3 et 20 | Codes 3 et 20 |
| Locales de session, cohérence aide/man et ELF rm séparé | 13 cas | 13 cas |
| Consultation publique et ELF séparé sans mutation P9 | SHA-256 et compteurs QMP | SHA-256 et compteurs QMP |
| Pagination, recherche, retour à l’invite et terminal 48 × 12 | Réussis | Réussis |
| Catalogue, grammaire, exécution de l’aide avec la console comme seule interface liée | 5 tests Darwin natifs | 5 tests Darwin sous Rosetta |
| P9/4096, version sans sonde, sortie du lecteur puis édition et sauvegarde, lecture après redémarrage | Réussi | Réussi |
| P8, lecture du témoin et documentation, SHA-256 de l’image avant/après | Identique | Identique |
| Mémoire privée de nsh après les consultations, mesurée par ps | 512 Kio | 532 Kio |

La mesure mémoire inclut la session, son historique et son allocateur. Le pic propre au lecteur pendant le rendu reste non mesuré. La recherche, la pagination et la sortie d’écran ont été exercées en QEMU. L’image UTM distribuée reste en v0.17.0.

### Contrôles de sécurité

- En session ordinaire, un véritable arrêt du service 9 renvoie 5. L’aide de `service stop` renvoie 0, décrit cette sous-commande et compte **zéro envoi STOP** au point d’émission IPC. Le service reste RUNNING et répond à NVFS. En maintenance, un arrêt réel déclenche la même sonde. Les cinq services enregistrés dans le profil installé portent les ID 1, 2, 9, 10 et 16. Le nom `storage-info` et l’ID 9 atteignent le même fournisseur. Les deux formes de l’aide générale et les aides de restriction et restauration ont été exercées.
- Le répertoire réel `/--version` est accessible par `cd -- /--version`. Depuis sa racine parente, `cd --version` imprime la version de nsh, renvoie 0 et conserve le même `pwd`. Aucun passage dans la résolution de `cd` pendant cette commande. Le contrôle positif entre dans le répertoire et déclenche la sonde.
- Après `rm --help` (0) et `rm --inconnue` (3), la première VM est arrêtée. L’outil NovaFS indépendant vérifie le volume et relit les sept octets de chacun des deux fichiers. Sur une copie de ce disque, `rm -- --inconnue` (0) retire exactement l’entrée demandée. Après arrêt de la seconde VM, la relecture trouve `NotFound` pour cette entrée seule. Le SHA-256 de `/voisin` est resté `26168da719b7251c9001fc856ffbdb4a148f7ac6a8bcc925116215242ca29f59`.
- La recette sur hôte lie le véritable traitement des options à une console de test, sans interface de fichiers, de processus ni de services. Dans les invités, le refus du véritable arrêt prouve l’absence du droit d’arrêt. Le banc en invité lance aussi un véritable ELF rm sans canaux et vérifie le résultat simultanément avec les compteurs du stockage. Les droits de lecture du volume demeurent disponibles à la session ordinaire.
- Le profil `cli-p8` conserve chaque image historique de test octet pour octet. Les neuf contrôles du registre natif réussissent, y compris la régénération et le dispatch des 48 sélecteurs. Les changements de l’index ABI généré portent sur ses références aux lignes sources. Le diff de NovaFS, du registre d’appels et du SDK est vide. NovaFS v0.9.13 conserve P8/P9 et le SDK nova-system reste en v0.2.0.

Les six entrées françaises exercées sont `fr`, `FR`, `fr-FR`, `fr-CA`, `fr-BE` et `fr-CH`. L’anglais a été observé pour `en-US`, `de-DE`, `es-ES`, `frr`, `fr--CA`, `zz` et la locale absente. Sur chaque parcours, les mêmes pages et verbes sont présents. Les profils français et anglais utilisent les mêmes programmes natifs.

Pour la mesure de non-écriture, le banc attend la fin du trafic initial du journal et conserve les relevés de repos. Chaque fenêtre compare le SHA-256 intégral du disque, les opérations et octets écrits, les FLUSH et les échecs de ces opérations. Le témoin positif `touch preuve-compteur` doit ensuite modifier l’empreinte et accroître le nombre d’écritures. L’arrêt de la VM intervient après ces mesures. La première tentative armée dès RUNNING a mesuré du trafic d’amorçage simultané sur les deux ISA. La recette finale consigne le repos avant son premier essai et ne rejoue aucune consultation ayant échoué pour la déclarer réussie.

| QEMU, session de qualification | SHA-256 intégral avant et après les consultations | Écritures / FLUSH supplémentaires | Témoin touch hors fenêtre |
|---|---|---|---|
| AArch64 | `d3bca6ad7a371c8510925eb399341f5c5409842b3312a45432255274aa6c66ef` | 0 / 0 | +21 écritures, +8 FLUSH |
| x86_64 | `6b95da24406cdbaf1b1d0db600675c80fd5240f134ea88569057840e7a73da28` | 0 / 0 | +21 écritures, +8 FLUSH |

En fr-CA, le noyau a lancé le véritable ELF rm, PID 30 pour la consultation d’aide dans chacune des deux VM. Le fils a signalé `lang=fr business=0` et a rendu le code 0. Les valeurs absolues avant/après sont 213 opérations d’écriture, 872 448 octets écrits et 77 FLUSH par VM. La qualification établit l’état du disque virtuel sur le profil exercé.

### Reproduction

Depuis le worktree de phase 1, configurer `NOVAOS_TMPDIR` et `TMPDIR` selon [AGENTS.md](../../AGENTS.md). Les répertoires de sortie des essais doivent être neufs. Construire préalablement l’outil hôte NovaFS avec `novaFS/lab/run build --offline` pour la cible Darwin ARM64.

```sh
for arch in aarch64 x86_64; do
  NOVA_DOC_TRACE=1 make -C novaOS ARCH="$arch" kernel || exit 1
  python3 -B novaOS/tests/documentation_cli.py "$arch" "../doc-qualif-$arch" || exit 1
  python3 -B novaOS/tests/documentation_locales.py "$arch" "../doc-langues-$arch" || exit 1
  make -C novaOS ARCH="$arch" kernel || exit 1
  python3 -B novaOS/tests/documentation_reprise.py "$arch" "../doc-reprise-$arch" || exit 1
done
python3 -B novaOS/tests/abi_registre.py -v
```

Les scénarios ciblés du mandat réussissent. Le banc historique complet `novaOS/tests/vi.py` s’arrête en secteurs de 512 octets sur `vi tronque` avant la suite de son parcours. Il attend « argument invalide » et obtient « capacité révoquée ». Le témoin construit depuis le tag v0.22.0 reproduit ce même résultat sur les deux architectures. La cause interne du diagnostic reste ouverte. Le parcours indépendant sur P9/4096 vérifie la sauvegarde et la reprise d’une édition valide. Les longues campagnes de stockage et les essais matériels restent hors de cette qualification ciblée.

Le banc d’isolation des temporaires, `tmp_prive.py`, a réussi quatre démarrages par architecture avec retrait, coupure de VM et reprise vérifiée par l’oracle. Le banc CLI historique `tmp_prive_cli.py` échoue en revanche sur `ls -a /` dès l’invite initiale (capacité révoquée, statut 10), sur les deux ISA. Le même banc et les mêmes paramètres reproduisent le refus avec le noyau du tag v0.22.0. La cause de ce refus reste à instruire. Les sorties des passages en échec sont conservées.

### Preuves et suite

Les dossiers locaux durables `OS-worktrees/preuves-documentation-0230-phase1` et `OS-worktrees/archives-documentation-0230-phase1` contiennent séries, sorties et codes, profils, images des essais et résultats de relecture. L’archive `documentation-bilingue-0230-phase1.tar.gz` regroupe 129 fichiers relus après archivage. SHA-256 **71513c2fefc690b809c2fcfa98ead1614f8e123d1ce24d198d64d9ecc14b64c0**. Le manifeste externe `verification-bilingue.json` contient une empreinte par fichier. `sources-qualification-finale.json` identifie également les modifications présentes pendant les essais entre les deuxième et troisième commits. Les répertoires `valid-cli`, `valid-locale` et `valid-reprise`, suffixés par architecture, contiennent les passages définitifs. Lors d’un essai antérieur, la trace série SMP a coupé la ligne du code de retour. La recette finale reconnaît le marqueur de statut réellement produit par le shell, indépendamment du début de ligne. Le premier essai de la sonde utilisait la sortie de débogage contingentée et ne détectait plus le témoin positif de résolution. Le passage retenu utilise une sonde console de qualification.

La prochaine extension proposée porte sur les autres commandes de fichiers puis les intégrées du shell, avec une preuve par nom, option et capacité. Les cas portant sur du texte littéral et les expressions de `test` exigent une recette propre avant la stable. La présente phase attend sa validation MOA/MOE. [Inventaire et historique](reconciliation-v0300/inventaire-commandes.md) · [Matrice serveur](reconciliation-v0300/acceptation-serveur.md)
