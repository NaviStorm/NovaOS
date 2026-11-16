# Clôture technique corrigée du catalogue documentaire, phase 1

## Décision proposée

Les critères documentaires des huit pilotes sont satisfaits sur QEMU AArch64 et x86_64. La clôture de phase 1 est proposée à la MOA/MOE. La base officielle est le tag annoté v0.22.0, commit `ce8f2d83792e4d83f6e4d7ffbce6400a288fbd27`. Le candidat initial publié `86080bbc5d615fdcd810d528659bff7e243aa130` est le parent du présent correctif sur `codex/documentation-0230-phase1`. Les versions du paquet de commandes et de nsh sont 0.20.0, le SDK est en 0.2.0, NovaFS en 0.9.13 et l’image UTM distribuée en v0.17.0.

## Cause et règle linguistique

Le sélecteur initial contrôlait la forme des sous-étiquettes. Il acceptait donc les cinq lettres de `zzzzz` comme variante. Le correctif vérifie, après le contrôle syntaxique, l’appartenance des écritures, régions et variantes aux tables IANA. Quatre lettres à la position de l’écriture forment un code de script. `Zzzz` est enregistré pour une écriture non codée. Cinq lettres à la position de variante sont syntaxiquement possibles et `zzzzz` est absente du registre. Les comparaisons ignorent la casse. La langue principale doit être exactement `fr`. Une locale absente ou rejetée sélectionne l’anglais. [Registre officiel](https://www.iana.org/assignments/language-subtag-registry/language-subtag-registry) · [RFC 5646](https://www.rfc-editor.org/rfc/rfc5646#section-2.2)

Les tables proviennent du registre du 17 septembre 2026, SHA-256 `755fad43283be7b41ebe3c89ad054b6eaf928f404f9c0edb74799e0eab74beb1`. Le générateur développe aussi les plages réservées. Aucun extlang du registre utilisé n’a le préfixe français. La politique vérifie l’enregistrement des variantes et conserve la grammaire existante des extensions et de l’usage privé. Elle ne vérifie pas la signification des valeurs d’extension auprès de leurs autorités. Une mise à jour des données IANA exige une régénération explicite et les tests du sélecteur. [Générateur hors ligne](../scripts/documentation_iana.py) · [Sélecteur](../../components/libraries/documentation_locale.rs)

## Contrôles exécutés sur le correctif

Deux invités QEMU, deux vCPU, 256 Mio chacun, secteurs de 512 octets. Chaque invité a conservé une seule session nsh pour les 16 cas. Les quatre consultations ont été exercées successivement pour chaque locale. La série contient la sortie complète de chaque commande et le statut rendu au shell. Le banc vérifie le texte de la page elle-même, la langue de l’interface du lecteur, puis son retour à l’invite.

| LANG | service stop --help, rm --help, man rm, man man | ELF autonome rm, aide et version | Statut dans les deux invités |
|---|---|---|---|
| fr-zzzz | Français sur les deux ISA | fr, PID 30 pour l’aide | 0 |
| fr-zzzzz | Anglais sur les deux ISA | en, PID 34 pour l’aide | 0 |
| fr-CA | Français sur les deux ISA | fr, PID 38 pour l’aide | 0 |
| fr-Zzzz | Français sur les deux ISA | fr, PID 42 pour l’aide | 0 |

Chaque ELF a signalé `business=0`. L’admission du fils documentaire couvre ses arguments privés, ses sorties console et ses ports métier nuls, puis omet la préparation du tmp. Les cinq tests du catalogue et de l’admission ont réussi sur Darwin ARM64 et x86_64 sous Rosetta. Les neuf tests du registre ABI ont réussi. Les 48 sélecteurs, l’ABI v1 et les sources NovaFS sont identiques à la base v0.22.0. [Banc invité](../tests/documentation_locales.py) · [Admission du fils](../kernel/src/sched/process/ipc/spawn.rs)

Les autres valeurs exercées sont `fr`, `FR`, `fr-FR`, `fr-BE`, `fr-CH` en français et `frr`, `fr--CA`, `en-US`, `de-DE`, `es-ES`, `zz` et la locale absente en anglais. Chaque fenêtre de consultation a conservé les compteurs de 213 écritures, 872 448 octets écrits et 77 FLUSH. Aucune requête STOP n’a été émise par l’aide. Le service 9 était toujours actif à la fin de l’essai. Le véritable STOP de maintenance du banc antérieur valide la sonde positive au même point d’émission.

| Invité | SHA-256 de P9 avant et après chaque fenêtre | Écritures / FLUSH de consultation | Témoin positif touch |
|---|---|---|---|
| AArch64 | `67790bc3220eeda020c2819c3ba3cba4d6f9028eddacca3265fafd146f6f9b76` | 0 / 0 | +21 écritures, +8 FLUSH, SHA différent |
| x86_64 | `a3ceda2703228a6d2a3787b226821bc5cd29da77f6f70d07985bbfaf5a2ea9be` | 0 / 0 | +21 écritures, +8 FLUSH, SHA différent |

## Qualification réutilisée et anomalies

Les huit noms `nsh`, `service`, `journal`, `ls`, `rm`, `cd`, `vi` et `man` ont une aide, une version identifiée et une page native. Le banc de locales rejoué contrôle leurs huit aides en français et anglais et leurs versions pour chaque locale. Les 71 observations initiales par ISA, les neuf ouvertures de pages, les options inconnues, le témoin de cd, la relecture indépendante après rm, P8 et la reprise de vi sur P9/4096 restent applicables. Le correctif change le choix de langue ; il conserve catalogue, lecteur, parseur de commandes, admission, ABI et stockage. Les mesures de mémoire initiales (512 et 532 Kio pour nsh) restent associées à leur révision. Aucun nouveau pic ni essai UTM n’est revendiqué. [Résultats initiaux](documentation-v0.23.0-phase1.md)

| Banc | Reproduction et écart | Références dans les 129 preuves | Classe et risque pour la phase 2 |
|---|---|---|---|
| vi.py | Sur QEMU AArch64 et x86_64, fixture P9/512, lancer `vi tronque` après le témoin UTF-8 invalide. Le fichier contient 4 095 octets ASCII et les deux premiers octets de 猫. Diagnostic « capacité révoquée » au lieu d’« argument invalide ». La suite du parcours s’interrompt à cet endroit. | Base v0.22.0 : `vi-reference-aarch64-rejeu.log`, `vi-reference-x86_64.log`. Candidat initial : `recette-vi-aarch64.log`, `recette-vi-x86_64.log`. | **Préexistante, cause non déterminée.** Risque modéré pour les diagnostics de lecture et les fichiers malformés. Garder un témoin de lecture valide et qualifier séparément les erreurs métier. |
| tmp_prive_cli.py | Sur les deux ISA, fixture P9/512 du banc tmp privé, lancer `ls -a /` dès la première invite. L’ELF rend 10 « capacité révoquée » avant la liste attendue. Les contrôles CLI ultérieurs de ce banc ne sont pas atteints. | Base : `temoin-tmp-{aarch64,x86_64}.log`. Candidat initial : `final-tmp-{aarch64,x86_64}.log` et séries des répertoires correspondants. | **Préexistante, cause non déterminée.** Risque élevé pour une recette dépendante du listage de la racine ou de délégations ordinaires. Utiliser aussi des relectures indépendantes d’objets nommés. Le simple succès de l’aide ne valide pas le listage métier. |

## Conservation et vérification

Sous le répertoire durable `OS-worktrees/`, les deux archives restent distinctes. L’audit a relu dans la première les 129 entrées régulières (177 804 399 octets), vérifié l’ensemble exact des noms et comparé chaque SHA-256 au manifeste. Parmi les copies de travail, 128 correspondent aussi octet pour octet. Le journal intermédiaire `livraison-cli-aarch64.log` a 3 831 octets, dont les 1 963 premiers sont exactement la copie archivée. Sa fin consigne un délai d’attente du banc. Sa version complète figure dans la nouvelle archive sous `complement-livraison-cli-aarch64.log`. Les résultats retenus demeurent ceux des séries `valid-*`.

| Fichier durable | SHA-256 |
|---|---|
| archives-documentation-0230-phase1/documentation-bilingue-0230-phase1.tar.gz | `71513c2fefc690b809c2fcfa98ead1614f8e123d1ce24d198d64d9ecc14b64c0` |
| archives-documentation-0230-phase1/verification-bilingue.json | `baf548fd64f6f11e744bdbab8e88aeb4e5a3affe299c1756d65d72cef7a4ad25` |
| archives-cloture-0230-phase1/cloture-phase1.tar.gz | `f4c407ca6d7b3be4107ffe3d1588891a5f0b740ad23bc8b9da864cc627327390` |
| archives-cloture-0230-phase1/SHA256SUMS | `418d126afe823b6b728c488a91c3de07d5d99953d424bb5c10c2a08d4d14c0c3` |

Les 24 entrées du complément ont été relues dans l’archive et comparées aux fichiers sources. Par invité, consulter `langues.json`, `langues.serial`, `langues.vm.json`, `repos-initial.json` et `temoin-ecriture.json`. Le profil, l’ISO démarrée, l’image P9 et les compteurs y sont conservés. Le manifeste `sources.json` permet de relier le correctif et les tests exécutés au contenu du commit.

Le contrôle Git a inventorié 22 worktrees avec leur statut, y compris les entrées ignorées. Le principal conserve sa modification de .gitignore et ses deux chemins non suivis. Les branches non intégrées, le worktree actif et les répertoires de preuves sont conservés. L’inactivité et la conservation de toutes les preuves des autres chantiers n’ont pas été établies pour une suppression. Aucun nettoyage effectué.

La [matrice des 29 commandes restantes](documentation-v0.23.0-phase2-plan.md) identifie leurs implémentations, leurs options, leurs droits et les témoins proposés. Elle prépare l’arbitrage sur quatre lots. La livraison reste limitée à la branche de phase 1.
