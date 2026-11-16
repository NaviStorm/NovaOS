# Preuves, reproduction et limites

Étude réalisée le 16 novembre 2026. Les sources NovaOS examinées sont `3bd662503b5fc6df0578e814b1e9d39243009dd4` sur `origin/main` et `09b2d96faeb4eb3c98881be99c245975c358abb6` sur la branche de documentation phase 2. Les deux références ont été lues sur origin. La seconde sert à documenter les 29 pages supplémentaires. Les preuves restent sous le répertoire durable `OS-worktrees/`, hors du dépôt principal et des temporaires.

| Élément | Résultat effectivement vérifié |
|---|---|
| Inventaire | Égalité d’ensemble entre les 37 lignes existantes et COMMANDS dans le répartiteur officiel. Couverture des 70 noms MOA (4 recouvrements), des 17 noms supplémentaires et des 2 ajouts proposés. 122 lignes uniques, aucune entrée name. Présence d’un lot et d’une référence pour chacune des 85 lignes supplémentaires. |
| Sources | Correspondance des SHA du rapport avec les relevés de références de 24 projets/dépôts ; neuf révisions GNU supplémentaires vérifiées par lecture de HEAD de l’extraction du tag et de COPYING. GNU coreutils est inclus parmi les 24. Pour uutils/diffutils le manifeste et le commit identifient l’instantané. Les URL non trouvées lors de la recherche restent visibles dans les index ; une réponse HTTP 404 n’a pas servi de preuve de licence ou d’option. |
| Comparaison hôte | Huit exécutions appariées GNU/Darwin (quatre comparaisons). Huit témoins système supplémentaires par architecture, tous conformes aux sorties et statuts attendus sur Darwin ARM64 et sous Rosetta. L’annexe indique aussi les limites d’extraction statique des paramètres. |
| Qualification NovaOS antérieure réutilisée | Les empreintes des archives lotfinal et regressions correspondent au [rapport du candidat ARC-026](https://github.com/NaviStorm/NovaOS/blob/09b2d96faeb4eb3c98881be99c245975c358abb6/novaOS/docs/documentation-v0.23.0-phase2.md). Ce rapport conserve les réserves sur vi tronqué, ls -a /, glob récursif, ls -latr / et l’intermittence x86_64/4096. Il ne qualifie aucun utilitaire futur. |
| Essais réservés au portage | Pas d’exécution NovaOS, de mesure mémoire invitée, de validation de performances ou de panne d’alimentation dans cette étude documentaire. Les services futurs, notamment droits complets, sauvegarde protégée, slots A/B et pare-feu distant, nécessitent un candidat exécutable et les deux campagnes définies au contrat. |

### Copies durables

- Répertoire de collecte : `OS-worktrees/preuves-arc-027/`.
- Archive indépendante du répertoire de collecte et du worktree Git : `OS-worktrees/archives-arc-027/etude-arc-027.tar.gz`, **462 fichiers**, 1 221 668 octets compressés, SHA-256 `fa915742e48bb137d21b70f1a4e4ac1d30a943239e8d3fb6231c48485f281252`.
- Manifeste par fichier : `OS-worktrees/archives-arc-027/SHA256SUMS`, SHA-256 `24a4d8ae1af7306a98566a995047e3c1dbbfc656e5b42a385a29c3e81bce669b`. Les 462 entrées ont été relues depuis l’archive et comparées au lieu de collecte. Ces deux copies locales se trouvent dans le même environnement de stockage ; aucune survie à sa perte physique n’a été qualifiée.
- Dans l’archive : `controle-inventaire.json`, `worktrees.txt`, `index.json`, `complement.json`, `sources-plus.json`, `perimetre.json`, `options-uutils.json`, sous-répertoires par projet, relevés Git et notices, `hote/comparaisons.txt`, `hote/oracles-2-isa.json`, aides et versions GNU, manuels hôtes et `reference-*.txt`.

### Vérification en lecture seule

Depuis le worktree durable de l’étude, la commande suivante compare les ensembles exacts de noms et d’empreintes. Les versions publiées des projets amont continuent d’évoluer ; pour une comparaison, extraire l’objet commit indiqué dans [sources.md](sources.md). Les observations hôte sont liées aux versions enregistrées dans l’archive.

```sh
python3 -B - <<'PY'
from pathlib import Path
import hashlib
import tarfile

archive = Path("../archives-arc-027")
original = Path("../preuves-arc-027")
def digest(stream):
    return hashlib.file_digest(stream, "sha256").hexdigest()

manifest = {}
for line in (archive / "SHA256SUMS").read_text().splitlines():
    value, name = line.split("  ", 1)
    assert name not in manifest
    manifest[name] = value
copies = {}
with tarfile.open(archive / "etude-arc-027.tar.gz", "r|gz") as stream:
    for member in stream:
        assert member.isfile() and member.name not in copies
        copies[member.name] = digest(stream.extractfile(member))
assert copies == manifest and len(copies) == 462
for name, value in manifest.items():
    with (original / name).open("rb") as file:
        assert digest(file) == value
print(len(copies), "fichiers vérifiés")
PY
```

Les contrôles de publication portent sur la branche de l’étude et sur le périmètre Markdown du diff. Le dépôt principal avec ses changements locaux et les autres worktrees sont conservés.

## Révision documentaire des arbitrages

Le contrôle compare les deux matrices au contenu Git de l’étude `09643693e250795ef6b17238710a29c8ba297a34`, ainsi que les 37 noms historiques aux pages Rust du commit `09b2d96faeb4eb3c98881be99c245975c358abb6`. Il compare chaque affectation de commande avec la liste du lot, puis les 18 listes avec les lignes de charge, de qualification et de calendrier. Il vérifie la structure des tableaux, les cibles des liens locaux et les références internes au calendrier et aux décisions. Les 462 preuves de l’étude sont relues suivant la procédure ci-dessus.

Le script `controle.py` et son relevé `resultats.json` sont conservés sous `OS-worktrees/preuves-arc-027-arbitrages/`, avec une seconde copie dans `OS-worktrees/archives-arc-027-arbitrages/arbitrages.tar.gz` et un manifeste `SHA256SUMS` dans le répertoire de l’archive. Depuis le worktree de cette révision, reproduire le contrôle par `python3 -B ../preuves-arc-027-arbitrages/controle.py`. Le relevé donne notamment les empreintes des documents contrôlés. Cette recette documentaire n’ouvre aucune VM. Les recettes de pipes, de permissions et des utilitaires restent à exécuter sur les futurs binaires.
