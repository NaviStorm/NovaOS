# Registre des qualifications de démarrage UEFI x86_64

| Jalon | Environnement et provenance | Résultat retenu | Limite de la preuve |
|---|---|---|---|
| [5W · STANDARD et RENFORCÉ](../diagnostic-uefi-5w.md) | QEMU Q35, OVMF, traces des périphériques, journal RAM et écran | STANDARD termine l’inventaire dans les deux modes Secure Boot. Les refus cryptographiques et le repli OVMF ont été observés. | Mesures sur périphériques émulés. Les garanties RENFORCÉ portent sur le PE et les reçus de ce profil. |
| [5X · Candidat figé](../diagnostic-uefi-5x.md) | Reconstructions sur hôte, copies sur fichiers ordinaires et refus de console sous QEMU | PE STANDARD reproductible, GPT/FAT32 à capacité exacte, refus 17 et arrêt CPU observé par le moniteur. | La décision préparatoire 5X maintenait A8 fermé. Les lectures internes du firmware précédaient déjà l’entrée EFI sur le banc. |
| [5Y · 28 novembre 2026 · premier démarrage physique](../diagnostic-uefi-5y.md) | Relevé du MSI GE62 6QC et vérification du support par la MOA | Premier EFI STANDARD x86_64 depuis la Kingston, Secure Boot inactif. Inventaire terminé, zéro activation, journal RAM, arrêt local et retour à Windows confirmé. **A8 clôturé pour cet essai unique.** | Noyau complet hors du parcours. Attribution VT-d ambiguë. Absence d’E/S internes et isolation DMA effective non démontrées sur le MSI. |
