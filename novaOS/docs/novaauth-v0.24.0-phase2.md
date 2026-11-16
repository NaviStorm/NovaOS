# NovaAuth v0.24.0, phase 2 — Registre d’identités et enrôlement initial

Le registre publie un ensemble cohérent de comptes, de groupes et de références de justificatifs. Les quatre profils QEMU ont chacun réussi les 23 scénarios de la recette. L'autorité indépendante assure la décision dans le laboratoire. Son remplacement par une autorité de production et le raccord à un authentificateur restent à qualifier.

## Références et périmètre

| Référence | Commit |
|---|---|
| Sources officielles v0.23.0 | `ea2d0c3cd5a38671ebdf263c8eeca72b3cabcff2` |
| Spécification de phase 1 | `39d96dd72fed057d1ddec086bb3d2aec0254c3fe` |
| Code et bancs de phase 2 | `909a97bb1c6c82554d6334b33ca25ed040eb7d06` |

Le profil ordinaire utilise les mêmes noyaux et services NovaFS, octet pour octet sur les deux architectures. La fonctionnalité Cargo explicite `novaauth-lab` compile le point d’entrée dans le service NovaFS de qualification. Le pilote de test utilise le canal bloc natif autorisé. Aucun sélecteur d’ABI et aucun champ disque P9 ne sont ajoutés. [Architecture de phase 1](novaauth-v0.24.0-phase1.md) · [Registre](../user/novaauth/src/lib.rs) · [Autorité de laboratoire](../user/novaauth/src/lab.rs)

## Première installation et droits

Le provisionneur doit détenir l’autorité exclusive de formater la cible et d’initialiser le domaine. Il vérifie une autorité vierge et lit tous les blocs pour établir que la cible est vide. Il crée les trois fichiers, puis lie leurs identifiants d’objet, le système de fichiers, le volume logique, le domaine et la référence de secours à la génération zéro. Seul le port d’enrôlement autorisé peut publier le premier administrateur. Un état absent, incohérent ou illisible après initialisation empêche l’activation. Un approvisionnement interrompu sur une cible devenue non vierge exige une intervention.

L’identité est indépendante du nom. La suppression conserve une trace de l’ancien identifiant et de son état. Le renommage conserve l’identité. Une recréation produit une nouvelle identité. Les références de justificatifs opaques n’accordent à elles seules aucun accès et restent distinctes après suppression. Les trois fichiers ne contiennent aucun mot de passe. Les points Rust `resolve_verified_factor` et `grant_attested` supposent un appelant interne de confiance ayant déjà vérifié le facteur ou le titulaire. Les exposer directement à un client permettrait l’usurpation. L’identifiant de secours est enregistré dès l’installation. Le protocole d’authentification forte du secours, la gestion des secrets et l’accès aux données surprotégées demeurent hors de cette implémentation.

Un droit octroyé inclut le titulaire et la décision exacte d’origine. La lecture d’un autre compte est réservée à l’administrateur. Chaque mutation vérifie à nouveau l’état courant. Tout changement de génération invalide aussi les droits du registre sans rapport avec ce changement. Cette granularité conservative exige leur réémission. La révocation des délégations de session dans les services consommateurs relève d’une intégration ultérieure.

## Publication et récupération

1. L’écrivain exclusif valide le droit, la génération attendue et l’identifiant de requête. Il calcule les trois nouvelles charges utiles et leur empreinte commune.
2. Chaque fichier de `/system/identity` (`users.db`, `groups.db`, `credentials.db`) dispose de deux emplacements logiques alternés de 16 Kio. L’écrivain prépare l’emplacement inactif et valide son écriture avec les transactions CoW de P9. Le lecteur sélectionne les objets par leur identité vérifiée.
3. L’écrivain relit les trois enregistrements, contrôle leur longueur, leur cohérence et leur empreinte, puis appelle `Authority::decide(previous, next)`. Cette comparaison porte notamment sur le domaine, le volume, les trois objets, la génération et le contenu. L’exclusivité doit couvrir aussi la préparation des trois fichiers.
4. Le succès est acquitté après synchronisation de la décision. Au démarrage, seule la décision désignée par `current()` autorise l’activation. Une autorité indisponible, un domaine incorrect ou une empreinte différente entraîne un refus. Après perte de réponse, `outcome(request)` permet de retrouver la décision durable. Une mutation `UNCERTAIN` ne déclenche aucune répétition automatique.

Le format applicatif emploie un en-tête binaire et des charges JSON bornées. Les maxima sont de 64 utilisateurs, fiches supprimées comprises, et 64 groupes. L’espace d’un emplacement peut être épuisé avant ces maxima. Le journal d’autorité du banc utilise 12 pages et effectue un parcours linéaire. Le fournisseur accepte au maximum 1 024 pages. L’exploitation prolongée, la rotation de l’autorité et la montée en charge exigent un travail distinct.

## Modèle de panne et résultats

QEMU 11.1.3, TCG, deux vCPU, machine AArch64 Cortex-A72 ou x86_64, secteurs VirtIO logiques et physiques de 512 ou 4 096 octets. Le raccord de laboratoire dispose de trois fichiers hôte distincts présentés par un descripteur VMDK plat. La vue P9 est bornée à son propre fichier. L’autorité et la commande de test résident dans les deux autres. Tous les supports de la campagne sont jetables.

| Contrôle | Résultat dans chacune des quatre configurations |
|---|---|
| Deux enrôlements en concurrence | Un administrateur publié, second enrôlement refusé. Après décision durable et perte de réponse, le même administrateur est retrouvé. |
| Deux créations fondées sur une même génération | Une publication et un conflit explicite. Deux requêtes sont simultanément en attente côté hôte puis traitées par l’écrivain unique invité. |
| Arrêt entre les écritures des bases ou après préparation, avant décision | Ancien ensemble intégral après redémarrage. |
| Arrêt après décision durable de création, avant réponse | Nouvel ensemble intégral et issue retrouvée par identifiant de requête. |
| Substitution à génération identique, corruption du contenu, mauvais domaine, autorité inaccessible ou vide | Refus d’activation. La suppression des trois bases d’un domaine initialisé ne rouvre pas l’enrôlement. |
| Restauration du P9 seul | L’autorité restée à jour fait refuser l’ancien état. |
| Restauration conjointe du P9 et de l’autorité | L’ancien état cohérent est accepté. Une révocation peut donc disparaître si les deux supports sont restaurés ensemble. |
| Cycle de vie et isolement | Renommage, retrait d’appartenance, désactivation, suppression et recréation du même nom vérifiés. L’ancien droit est refusé. |

Le banc attend le marqueur de coupure dans l’invité puis tue QEMU avant de relire les supports au démarrage suivant. La recette finale restaure la même image de départ avant chaque coupure de préparation. Sans cette précaution, une écriture identique déjà présente peut légitimement éviter le FLUSH visé. Les premiers essais restent archivés. Les arrêts de QEMU ne qualifient ni la perte physique d’alimentation ni la disponibilité après déchirure d’une ancre P9. Le chaînage SHA-256 du journal suppose un hôte de confiance. Une autorité TPM ou externe, protégée contre le rollback, et un dispositif de récupération fortement authentifié restent à fournir avant un déploiement.

Deux tests de contrat Rust ont réussi pour chacune des cibles Darwin AArch64 et x86_64. Ils complètent les boots QEMU, notamment avec une réponse `UNCERTAIN` injectée après décision, l’absence de répétition, la protection du dernier administrateur et le refus des mauvaises cibles. Les liaisons d’un fichier privé, d’un cache et d’une tâche à un ancien titulaire ont été testées par témoins en mémoire. L’isolement de ces trois consommateurs avec des comptes NovaAuth réels reste à qualifier. La concurrence du registre utilise deux appelants hôte et un écrivain invité. Elle ne qualifie pas deux processus clients IPC indépendants.

## Régressions et mesures

Les neuf contrôles de cohérence du registre ABI v1 réussissent pour ses 48 sélecteurs. Sur chacun des deux profils ordinaires, les campagnes des pipes `cat | wc` (512 octets) et `cat | head` (4 096 octets) réussissent, ainsi que le banc actuel de supervision sur P9/4096, y compris retrait puis restauration des droits. Le banc historique `services.py` échoue sur les deux noyaux de base. La comparaison indépendante de son catalogue identifie une seule entrée ajoutée, le journal persistant `/\x1fNovaOS-journal`, sans entrée supprimée ou modifiée. L’oracle historique l’inclut dans le catalogue attendu inchangé. Ce résultat reste ouvert. Les noyaux ordinaires reconstruits sont identiques à la base par comparaison binaire et les trois marqueurs de qualification en sont absents.

| Invité / secteur | Provisionnement | Deux demandes de création | Relecture de l’issue | Cycle de révocation | Pic supplémentaire du tas |
|---|---:|---:|---:|---:|---:|
| AArch64 / 512 | 432,2 ms | 105,3 ms | 83,3 ms | 318,9 ms | 64 Kio |
| AArch64 / 4 096 | 369,1 ms | 95,9 ms | 84,8 ms | 321,4 ms | 64 Kio |
| x86_64 / 512 | 323,1 ms | 207,0 ms | 92,1 ms | 315,5 ms | 64 Kio |
| x86_64 / 4 096 | 322,6 ms | 89,4 ms | 88,0 ms | 320,5 ms | 64 Kio |

Chaque durée est une observation côté hôte du scénario entier entre les marqueurs série, après l’amorçage. Les campagnes ont pu s’exécuter en parallèle. Ces mesures ne fournissent pas de percentiles ni de latence isolée par transaction. Le pic mesure les extensions du tas du service après le début du scénario. Il exclut la mémoire déjà allouée, et les scénarios interrompus ne rapportent pas leur pic. Aucun résultat UTM ou matériel physique n’est revendiqué.

## Reproduction et archives

Le [banc invité](../user/novaauth/lab/qemu.py) accepte l’architecture, le secteur, l’ISO du laboratoire et un répertoire de sortie **nouveau**. Depuis `novaOS/`, avec les cibles Rust du projet installées, `TMPDIR` et `NOVAOS_TMPDIR` configurés selon les consignes, et `PREUVES` désignant un répertoire durable distinct, exécuter pour chaque valeur de `NOVA_ARCH` (`aarch64`, `x86_64`) :

```sh
export NOVA_ARCH
. scripts/target.sh
rustup run stable cargo build --release --locked --offline -p novaos \
  --target "$rust_target" --features novaauth-lab --target-dir build/novaauth-target
export NOVA_KERNEL_ELF="$PWD/build/novaauth-target/$rust_target/release/novaos"
sh scripts/iso.sh cli "$PREUVES/qual-$NOVA_ARCH.iso"
for secteur in 512 4096; do
    python3 -B user/novaauth/lab/qemu.py "$NOVA_ARCH" "$secteur" \
      "$PREUVES/qual-$NOVA_ARCH.iso" "$PREUVES/recette-$NOVA_ARCH-$secteur"
done
```

La [sélection publiée](validation/novaauth-phase2-v0.24.0.tar.gz) contient 110 fichiers dont les 92 sorties série, les résultats structurés, les mesures, le contrôle des binaires et le manifeste intégral. Son SHA-256 est `4f17ad650c8fd592f232a8d828433463dd559d48d2909261cdb9918ce7c5c72d`.

La copie durable complète est `OS-worktrees/archives-novaauth-phase2-0240/qualification.tar.gz`, à côté du worktree. Elle contient 560 fichiers et leur manifeste, y compris les ELF, ISO, volumes synthétiques et premières tentatives. Les 561 membres ont été décompressés et comparés par SHA-256 aux fichiers originaux. Le manifeste `SHA256SUMS` a pour empreinte `c678d96b90e6b4062fbe687160444e9bb020021b2f099383be43c62e41c132f1`. L’archive complète de 170 278 907 octets a pour SHA-256 `68882ce7e68457ff53a5848b747c693e9d36fe14c755fd8143879aeb42d2e48b`.
