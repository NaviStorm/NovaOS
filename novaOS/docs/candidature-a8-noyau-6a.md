# Candidature A8 · un démarrage du noyau sur le MSI GE62 6QC

**Décision proposée à la MOE : candidature prête pour arbitrage, A8 noyau fermé.** L’essai envisagé s’arrête au verdict terminal du noyau. La phase 6A a utilisé des fichiers ordinaires et QEMU. L’ancienne autorisation du diagnostic STANDARD est clôturée.

| Élément à figer dans le futur mandat | Valeur |
|---|---|
| Machine et nombre d’essais | MSI GE62 6QC, un seul démarrage ponctuel depuis le support désigné |
| Support | Kingston DataTraveler 2.0 déjà identifiée par la MOA, capacité exacte 4 005 560 320 octets |
| Image brute candidate | `9945b4334170495d9a9836f1f8c9224c8293600b33445fc8f7c4d799ba26961a` (SHA-256 de tous les octets, GPT secondaire à la LBA 7 823 359) |
| Profil | Même image générique x86_64 pour les PC compatibles. Secure Boot déjà **désactivé** sur le MSI selon la décision MOA |
| Résultat nominal à relever | Entrée noyau, mémoire initialisée, ordonnanceur actif, nombre de CPU au rendez-vous, puis `VERDICT OK - arret local` |
| Résultat conservé | Photo ou transcription de l’écran par la MOA. Journal noyau volatil en RAM. NOVALOGS reste vide. |

## Risques STANDARD déjà acceptés

Le chargement antérieur peut avoir eu lieu sans protection DMA attestée. Le firmware peut lire les disques internes avant l’entrée EFI et lancer Windows après un refus. Avec Secure Boot désactivé, la confiance dans le premier EFI repose sur la vérification externe et la maîtrise du support. Le retour à Windows ne démontre pas l’absence d’E/S. La garde réseau/stockage reste applicable.

## Risques propres au noyau 6A à accepter séparément

- **Sortie UEFI.** Après `ExitBootServices`, le firmware cesse de fournir les services de démarrage. Une carte mémoire ou un relais erronés peuvent empêcher l’entrée noyau ou l’affichage du premier jalon. Un arrêt à ce stade peut imposer une extinction manuelle.
- **Mémoire et SMP.** Le noyau installe ses tables, ses interruptions, son allocateur et ses piles, lance les CPU secondaires et exerce l’ordonnanceur. Un écart de topologie, de minuterie ou d’acquittement peut laisser le panneau incomplet. Les quatre CPU actifs du relevé MADT MSI 5Y constituent la référence opérateur pour le rendez-vous.
- **Framebuffer.** Le profil utilise les pages vidéo et les traductions décrites par Limine. Les droits CPU, le pas et les bornes sont contrôlés. L’association physique au panneau actif, les GPU multiples et les DMA hérités restent propres à la machine. Un écran noir ou inchangé donne un résultat indéterminé. Sur un PC sans série accessible, le journal RAM disparaît à l’extinction.
- **Stockage.** Les compteurs SATA et la stabilité du disque témoin sont observables sous QEMU. Sur le MSI, l’opérateur ne dispose pas d’un témoin équivalent. Le scénario s’arrête avant les pilotes internes et ne prend ni entrée clavier ni commande interactive.

## Portes opérateur du futur mandat

1. La MOA confirme l’identité de la clé, sa disponibilité pour une réécriture et l’autorisation distincte de préparation et de démarrage. La préparation remplacera le diagnostic 5Y sur cette clé. Vérifier une sauvegarde Windows récupérable et la disponibilité privée du moyen de récupération BitLocker.
2. Vérifier la provenance du commit et le SHA-256 du fichier brut. Après une éventuelle écriture autorisée, relire toute la clé : capacité, empreinte ci-dessus, deux GPT et deux FAT32. Tout écart annule la candidature. La présente phase ne demande aucune préparation physique.
3. Confirmer l’état Secure Boot convenu et utiliser le choix de démarrage ponctuel prévu par le propriétaire. Conserver l’ordre d’amorçage permanent, le BIOS, les clés UEFI et la configuration Windows.
4. Photographier les jalons et le nombre de CPU. À tout refus, nombre inattendu, absence de verdict, écran illisible ou repli vers Windows, consigner le dernier témoin visible et interrompre l’essai. Une répétition nécessite une nouvelle décision.
5. Après le relevé, effectuer l’extinction contrôlée prévue par la MOA. Le verdict terminal stationne les processeurs et laisse le panneau visible. Le retrait de la clé éteinte et le démarrage volontaire ultérieur de Windows terminent le constat opérateur.

[Qualification QEMU et limites](live-noyau-6a.md) · [Image et empreintes](validation/live-noyau-6a/SHA256SUMS)
