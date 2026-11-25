# NovaAuth R2 — E2 et concurrence entre EXEC et retrait de G

Référence duplex `777ff386446d70f38c17752275a6fbdd73a98377`. Le premier raccord E2 est publié dans `4c21fd7d2451be82d8a1bc35cb0ace97ab271564`. Les seize essais ci-dessous utilisent la correction de drainage et les sondes supplémentaires du présent dossier. Chaque variante possède ses propres empreintes de noyau, de registre, de shell et d’ISO. Le volume initial est identifié dans chaque résultat.

| Recette | Situation provoquée | Issue observée sur les quatre profils |
|---|---|---|
| E2 | Bob présente son ancien ticket H après le retrait durable de G | Statut 0, enfant 23, 14 octets conformes, stderr vide, quatre E/S avec admissions et terminaux individuels. |
| E7 G | Fermeture de la porte encore en phase Requested, avant admission distante | Statut 10 avant échéance, aucun enfant. La position 14 réserve l’annulation de la demande. |
| E8 G | Fermeture après admission durable du lancement, avant création | Admission 13, réservation du retrait 14, annulation irréversible 15. Statut 10 avant échéance, aucun enfant. |
| E8 H | Admission H antérieure au retrait, création différée après confirmation du retrait de G | Un enfant 23 effectivement créé, 14 octets conformes et quatre issues VFS. |

Le statut 10 est `REVOKED`. Le contenu témoin rendu à Bob est `T:A\tB\tC\nD\tE\tF\n`, avec tabulations et sauts de ligne réels. Les résultats corrèlent chaque issue à l’opération de l’autorité indépendante. Les trois classes 1, 2 et 4 ont chacune un acquittement durable avant `RETIREE`. Le dossier de chaque essai brut et ses collectes privées sont conservés localement. Les JSON publics donnent leurs empreintes. Les traces publiques contiennent seulement des événements sélectionnés et les mesures.

Le premier E8 G a mis en évidence un défaut de progression du registre vivant. Une annulation locale irréversible et son terminal distant avaient été enregistrés, mais le coordinateur renvoyait une attente avant d’avoir traité le terminal de cette même opération. La correction reprend le traitement de la feuille déjà admise après observation de son annulation effective, vérifie le reçu correspondant puis règle la porte. Une opération indéterminée reste en attente. La reprise à froid et les règles d’activation demeurent inchangées.

## Tailles, pics et périmètre

La mesure des déclarations compilées pour chaque cible précède les recettes. Le pool utilise 32 octets, le fichier ouvert 32, l’échange 272. Le sous-total du raccord est de 4 090 octets, les deux compteurs de pic prennent quatre octets, soit 4 094 sur 4 096. La sonde couvre chaque prélèvement de cadre physique et les variations d’occupation des pages IPC dans la fenêtre instrumentée entre la préparation du ticket de Bob et le drainage final. Au dernier point, le nombre de cadres IPC vivants est vérifié contre l’état réel du pool. Le pic atteint 74 cadres IPC. Le minimum de cadres physiques libres est compris entre 59 025 et 59 027 sur x86_64 et entre 58 502 et 58 506 sur AArch64. Les échantillons par acteur et par phase montrent au plus quatre racines. Endpoints, nœuds, espaces et cadres IPC finissent à zéro. Les temps hôtes d’attente et d’exécution sous verrou figurent séparément par essai.

P9/512 emploie les fixtures compactes indiquées dans le dossier E2 précédent. La fixture complète AArch64/P9-512, la réserve historique de 301 cadres instrumentés AArch64 et la reproductibilité entre environnements restent ouvertes. L’admission H historique 19 demeure en quarantaine sous son identifiant original. Aucun essai de ce dossier ne résout cette admission. La contention de deux enfants fichier sur le dernier emplacement constitue une recette distincte.

## Relecture

`shasum -a 256 -c SHA256SUMS` vérifie les fichiers publics depuis ce dossier. Pour reconstituer les sources d’un banc, lancer `python3 -B reconstruire.py E8G "$TMPDIR/export-e8g-neuf"` dans un environnement de construction NovaOS. La destination doit être neuve et extérieure aux worktrees. Les quatre scripts de banc archivés conservent les chemins de la campagne.
