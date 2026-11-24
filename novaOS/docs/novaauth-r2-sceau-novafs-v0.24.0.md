# NovaAuth v0.24.0 — Relecture NovaFS de la portée EXEC

25 novembre 2026, heure de Paris. Base `19cfbf6029bb5a0f3124088292728aa87a8c4c46`.

## Code et périmètre

Le commit `46ea56a874c4b034f9620b24491ceaa021cfbde5` ajoute la relecture du sceau de la portée depuis le volume P9 monté. Le chemin précédent reconstruisait le sceau depuis les métadonnées immuables du ticket et le comparait à la vue NovaAuth. Les mêmes identités sont maintenant confrontées à la marque effectivement lue par NovaFS, au type des objets et à la parenté fichier–dossier.

La trame privée `NXEXFS01` occupe 128 octets. Elle lie l’identifiant d’opération, les deux identifiants du volume, le titulaire, le dossier, le fichier et l’instance NovaFS. Le registre utilise la conversation privée existante après vérification de la tête. La réponse lie la requête complète, l’instance et les 40 octets du sceau. Le registre vérifie encore l’empreinte du résultat et les révisions de politique sur la vue authentifiée. La sonde se limite aux métadonnées de la portée et ne lit aucun contenu de fichier.

Le raccord n’est injecté que dans l’export de compilation sélectif. Il ne crée aucune racine IPC. Les formats existants, les 128 octets par trame et les 48 sélecteurs ABI v1 sont préservés. Il n’ouvre aucun chemin vers l’autorité locale.

## Qualification réellement effectuée

Le témoin, ajouté uniquement à l’export, utilise la vue P9 activée par le registre invité et une fixture déjà provisionnée. La sonde vérifie le vrai sceau, puis envoie une requête pour un autre fichier et une autre pour un autre volume. L’oracle exige les statuts natifs et l’absence de création de processus. Chaque essai vérifie l’ISO `novaauth-sessions` et l’empreinte du noyau inclus. Les témoins Direct ne sont pas rejoués.

| Profil | Sceau attendu | Autre fichier | Autre volume | Racines max observées | Pages registre / signataire |
|---|---:|---:|---:|---:|---:|
| x86_64/P9-512 | 0 | 10 | 10 | 4 | 165 / 73 |
| x86_64/P9-4096 | 0 | 10 | 10 | 4 | 165 / 73 |
| AArch64/P9-512 | 0 | 10 | 10 | 4 | 162 / 65 |
| AArch64/P9-4096 | 0 | 10 | 10 | 4 | 162 / 65 |

Sur chaque profil, `10` signifie `REVOKED`. Aucune admission EXEC ni mutation administrative. Les mesures dans la phase d’activation indiquent sept handles pour le registre, six pour le signataire et zéro attente à la fin. Les deux instantanés font état de 31 cadres IPC globaux. Ils ne mesurent pas un pic pendant un lancement. Le relais hôte est drainé et les quatre verrous sont libérés. Le banc prend 36,892 s pour les quatre démarrages, avec environ 1 ms d’attente cumulée du verrou. Le premier démarrage prévu s’est arrêté avant QEMU, car `spawn` relançait l’entrée du script hôte. L’échec est conservé et l’entrée a été protégée.

Neuf tests natifs réussis sur chaque cible Darwin (dont trois pour le sceau). Le cas de parenté utilise les moteurs P9/512 et P9/4096 sur disques de test. Le déplacement réel du fichier hors du dossier annule la preuve. Le coordinateur et le calcul de layout passent 17 tests par cible. Le sous-total statique reste **4 026 / 4 096 octets** sur les deux cibles. Les neuf tests ABI passent. L’avertissement historique `rust-objcopy` concernant `libLLVM.dylib` est conservé.

## Matrice consolidée

| Recette | x86_64/512 | x86_64/4096 | AArch64/512 | AArch64/4096 |
|---|---|---|---|---|
| Q1 VFS via `nsh`, Direct/G/H | Historique | Historique | Historique | Historique |
| E1 ancien ticket Direct | Fixture compacte | Fixture compacte | Fixture compacte | Fixture compacte |
| Relecture NovaFS du sceau EXEC | Réussie | Réussie | Réussie | Réussie |
| E2 consommation ancienne H et E3 refus ancien G | Non exécutés | Non exécutés | Non exécutés | Non exécutés |
| Q2/Q3 opérations VFS et retraits concurrents déjà couverts | Historique | Historique | Historique | Historique |
| Q2/Q3 admission EXEC G et drainage | Non exécutés | Non exécutés | Non exécutés | Non exécutés |
| Q4 réponse perdue après ACK durable | Historique | Historique | Historique | Historique |
| Q4 coupure après réservation, avant publication | Refus sûr historique | Refus sûr historique | Refus sûr historique | Refus sûr historique |
| Q4 poursuite d’un retrait incomplet après disparition d’A | Contrat soumis | Contrat soumis | Contrat soumis | Contrat soumis |

Le chargeur isolé fournit encore une portée vide. La remise d’un ticket avec portée garde son refus sûr. Il manque le passage de la décision authentifiée aux canaux fichier de l’enfant, le raccord des admissions individuelles à ces canaux et l’inspection des tickets dormants, des lecteurs préparés et des délégations d’enfants pour le drainage. Les trois acquittements restent distincts. La preuve de relecture ne vaut pas preuve d’octroi ou d’admission.

Les réserves de **301 cadres instrumentés AArch64**, de **reproductibilité entre environnements** et d’**expiration d’EXEC avec la fixture complète AArch64/P9-512** restent ouvertes. Le témoin de sceau ne consomme aucun ticket, même sur la fixture complète.

[Preuves et commits des recettes historiques](novaauth-r2-ancien-direct-v0.24.0.md#matrice-consolidée-q1q4).

## Reprise froide

Le texte intégral du contrat publié dans `1fdf10d66a44964c4f44abd3bb85082643be29a3` est repris à l’identique dans l’archive. Il reste soumis à arbitrage. Aucune de ses nouvelles règles n’est implémentée. [Texte intégral pour revue MOA/MOE](novaauth-r2-reprise-froide-proposition-v0.24.0.md).

[Résultats](validation/novaauth-r2-sceau-novafs-v0.24.0/resultats.json) · [Archive](validation/novaauth-r2-sceau-novafs-v0.24.0/archive.json) · [Manifeste SHA-256](validation/novaauth-r2-sceau-novafs-v0.24.0/MANIFEST.sha256).
