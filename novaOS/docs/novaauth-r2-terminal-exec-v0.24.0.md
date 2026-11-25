# NovaAuth v0.24.0 — Annulation EXEC et conservation des preuves

25 novembre 2026. Branche `codex/novaauth-administration-0240-phase1`. Référence `9303e45ccf23e8687d1a30588c8bc3ba83c10fba`. Code invité `4028b32b96a9447f2d21392261b2c6151a93a2f8`, avec instrumentation QEMU consignée dans les preuves.

## Réconciliation EXEC après admission

Le registre marquait le terminal comme envoyé lors de la demande de tombe, destinée aux annulations sans admission. Quand l’autorité refusait la tombe parce qu’une admission existait et que la consultation suivante se perdait, la pompe pouvait rester en consultation indéfiniment alors qu’aucun `FINISH` n’avait été envoyé. Le correctif distingue les deux soumissions. Un reçu authentifié établissant que la même admission est toujours en attente permet le premier `FINISH` après la tentative de tombe. Après envoi de `FINISH`, seule la consultation par identifiant est permise en cas d’issue inconnue.

Le témoin utilise le vrai `nsh` et un ELF installé sans portée fichier. Le registre extrait une admission signée réelle, puis déclenche l’annulation noyau avant création par le raccord d’essai. L’autorité établit le refus de la tombe, puis le banc supprime la première réponse de consultation `Pending` et la réponse du seul `FINISH` après commit. Le noyau confirme la situation terminale à partir de la réconciliation. Les traces de l’autorité et celles du noyau permettent de suivre le même lancement.

| Profil QEMU | Statut natif | Marge avant l’échéance | Enfants | Admission → terminal durable | Durée du banc |
|---|---:|---:|---:|---:|---:|
| x86_64/P9-512 | `REVOKED` (10) | 4 212 ms | 0 | 7 → 8 | 10,52 s |
| x86_64/P9-4096 | `REVOKED` (10) | 4 212 ms | 0 | 7 → 8 | 10,52 s |
| AArch64/P9-512 | `REVOKED` (10) | 3 824 ms | 0 | 7 → 8 | 19,32 s |
| AArch64/P9-4096 | `REVOKED` (10) | 4 232 ms | 0 | 7 → 8 | 16,24 s |

Les marges sont calculées exclusivement avec l’horloge de l’invité à 250 Hz. Les positions appartiennent à une autorité de test distincte pour chaque profil. Chaque cas effectue une tentative de tombe et **un seul** `FINISH`. Aucun enfant n’est identifié dans le reçu ni dans la trace noyau. Ces essais couvrent le terminal d’une admission déjà engagée. Ils ne couvrent aucun retrait réel de G.

## Ressources effectivement observées

| Mesure du témoin | Avant émission | Consommation | Fermeture | Après collecte ordinaire |
|---|---:|---:|---:|---:|
| Racines du `nsh` | 1 | 2 | 1 | — |
| Handles du `nsh` | 8 | 9 | 8 | — |
| Endpoints globaux | 56 | 57 | 56 | — |
| Nœuds IPC globaux | 117 | 119 | 117 | — |
| Processus vivants | 17 | 17 | 17 | — |
| Cadres IPC globaux | 65 | 66 | 66 | 65 |

Les quatre traces montrent la collecte naturelle `66 → 65` après fermeture du ticket. La sonde journalise l’appel existant à la collecte, sans en déclencher un nouveau. Les cadres libres globaux varient avec les autres processus et restent consignés séparément. Le registre revient à sept handles, le signataire à six. Les deux restent à quatre racines, avec constat du refus d’une cinquième. Le registre occupe 164 pages sur x86_64 et 162 sur AArch64, le signataire 73 et 65. Ces relevés comprennent les instantanés d’activation et le témoin Direct. Le pic de pages d’un vrai parcours EXEC G/H demeure à mesurer.

Le sous-total statique additionnel mesuré est de **4 026 / 4 096 octets** sur ARM64 Darwin et x86_64 sous Rosetta. Il garde une marge de 70 octets. Il exclut les piles TLS et le cache P9. Les sources du noyau, de l’ABI, des utilitaires et de NovaFS P9 sont identiques à la référence. Les neuf contrôles ABI passent et les 48 sélecteurs sont conservés. Les ELF historiques ne sont pas reconstruits pour cette qualification ciblée. Les résultats historiques restent liés à leurs profils et artefacts publiés.

## Matrice consolidée

| Recette | x86_64/512 | x86_64/4096 | AArch64/512 | AArch64/4096 |
|---|---|---|---|---|
| Q1 VFS Direct/G/H via `nsh` | Historique | Historique | Historique | Historique |
| E1 ancien ticket Direct réellement consommé | Fixture compacte | Fixture compacte | Fixture compacte | Fixture compacte |
| Relecture de la portée scellée par NovaFS | Historique, métadonnées | Historique, métadonnées | Historique, métadonnées | Historique, métadonnées |
| Annulation EXEC après admission, pertes et restitution | Réussie | Réussie | Réussie | Réussie |
| E2 ancien ticket H utilisé après retrait G | Non exécutée | Non exécutée | Non exécutée | Non exécutée |
| E3 ancien ticket G refusé par retrait avant échéance | Non exécutée | Non exécutée | Non exécutée | Non exécutée |
| Q2/Q3 opérations VFS en vol et concurrence | Historique | Historique | Historique | Historique |
| Q2/Q3 admission EXEC dépendante de G et drainage concurrent | Non exécutée | Non exécutée | Non exécutée | Non exécutée |
| Q4 réponse après ACK durable perdue | Historique | Historique | Historique | Historique |
| Q4 coupure après réservation avant publication | Refus sûr historique | Refus sûr historique | Refus sûr historique | Refus sûr historique |
| Q4 retrait incomplet et ancienne incarnation disparue | Quarantaine de référence | Quarantaine de référence | Quarantaine de référence | Quarantaine de référence |

[Preuves E1 et Q1–Q4](novaauth-r2-ancien-direct-v0.24.0.md#matrice-consolidée-q1q4) · [Preuve de relecture NovaFS](novaauth-r2-sceau-novafs-v0.24.0.md).

## Raccords EXEC restant à terminer

1. Le chargeur isolé transmet encore zéro pour le dossier, le fichier et le répertoire de travail. Il faut lier au dossier scellé la permission réellement requise par l’installation, avec sa provenance exacte et ses révisions. Le descripteur doit être validé avant émission d’un ticket utilisable.
2. Les canaux fichier de l’enfant existent dans le chemin historique, dont le lecteur sélectionne une décision locale Direct. Le raccord isolé doit traiter chaque requête reçue du vrai enfant sur ces canaux avec la décision authentifiée du registre et son admission durable individuelle. Le chemin historique ne peut pas servir de repli pour une portée G/H. Les opérations en file, les effets produits et la remise d’un résultat doivent conserver leurs propres preuves.
3. Le retrait doit fermer les tickets et les capacités fichier issus de G, y compris les lecteurs préparés, sous la sérialisation des admissions. L’inspection locale doit remettre les identités complètes des opérations à drainer. La conservation de H doit être confirmée par la création effective de l’enfant et une lecture via ses canaux fichier. Les trois acquittements restent à vérifier dans ces conditions, avec mesure du vrai pic G/H.
4. Le contrôle automatique a rejeté une tentative de modifier la sélection de route de la pompe. La modification retirait un garde `UNCERTAIN` en reprenant la feuille conservée sans preuve de liaison ni test préalable. Le diff a été vérifié après le rejet. Le code de sélection reste identique et l’effet de cette concurrence sur la progression n’a pas été reproduit dans l’invité. Toute reprise de ce point exigera une preuve de liaison de la feuille à son propriétaire, sa session, son opération et sa génération, ainsi qu’une vérification autorisée sur deux routes.

## Reprise froide et points à arbitrer

Le complément recommande de conserver dans le journal indépendant les faits originaux du VFS, des délégations et d’EXEC, avec l’ordre durable, les identités des auteurs et la borne des opérations couvertes. Un journal local authentifié peut compléter les faits de stockage sur un disque survivant, mais a besoin d’une ancre extérieure contre la restauration. Une clôture ordonnée par un composant extérieur pourrait réduire la fenêtre d’incertitude si ce composant contrôlait réellement les effets concernés. Ce serait un changement distinct de frontière de confiance.

Pour le coût, les formats actuels donnent 241 octets pour une admission VFS et 194 pour son terminal, hors signatures, indexes et journal. Le coût d’une preuve de fermeture conserve une écriture durable par classe, avec 161 octets de requête par acquittement existant. La voie locale VFS utilise une trame de 128 octets dans chaque sens, sans nouvelle racine prévue. Pour les délégations, aucun octet de données utilisateur ne transite et le futur coût de rétention varie avec le nombre de références et de descendants à couvrir. Pour EXEC, un descripteur avec une dépendance fichier vaut 656 octets avant signature, l’admission sélective R2 porte 312 octets et `FINISH` en porte 313 avant les enveloppes de transport. Le stockage réel, les piles et la latence de persistance restent à mesurer par classe.

Restent à arbitrer la granularité et la rétention des preuves originales, ainsi que toute solution de clôture extérieure ou d’activation sans fait de classe original. Une lecture de P9 peut établir un effet de publication si l’opération y est liée de façon unique. Elle ne règle ni une création EXEC non attestée ni une fermeture de délégation non démontrée. Après perte d’un tel fait, l’incarnation demeure en quarantaine. Aucune règle de substitution ou de réactivation n’a été ajoutée. [Complément d’architecture](novaauth-r2-preuves-avant-coupure-v0.24.0.md) · [Contrat examiné](novaauth-r2-reprise-froide-proposition-v0.24.0.md).

## Contrôles et archives

Les 19 tests hôtes de l’autorité passent. Les 19 tests natifs de transitions, de coordination et de layout passent sur chacune des deux cibles Darwin. Les quatre essais finaux prennent 56,60 s, et les bancs ayant démarré QEMU totalisent 179,90 s avec les premiers essais conservés. Attente cumulée du verrou de 0,0037 s, aucune concurrence incompatible et libération enregistrée après chaque essai. Les compilations, le diagnostic hors banc et le scellement sont en dehors de ces durées. Le montage de l’ISO `novaauth-sessions` et l’identité de son noyau sont vérifiés sur les deux architectures.

Les premiers essais ont isolé une table hôte non initialisée avant lancement, puis une invite prématurée. L’essai de restitution initial compare le compteur juste avant la collecte IPC ordinaire. Il est conservé avec le compteur supplémentaire et la mesure ultérieure de sa libération. Les traces de série archivées masquent les références numériques de tickets et conservent l’empreinte des originaux locaux. Aucun disque, binaire ou secret de laboratoire n’est inclus. Les 52 preuves archivées du témoin NovaFS précédent ont été relues intégralement.

Les réserves de 301 cadres instrumentés AArch64, de reproductibilité entre environnements et de fixture complète AArch64/P9-512 restent ouvertes. R2 reste ouvert.

[Résultats détaillés](validation/novaauth-r2-terminal-exec-v0.24.0/resultats.json) · [Métadonnées de l’archive](validation/novaauth-r2-terminal-exec-v0.24.0/archive.json) · [Manifeste SHA-256](validation/novaauth-r2-terminal-exec-v0.24.0/MANIFEST.sha256).
