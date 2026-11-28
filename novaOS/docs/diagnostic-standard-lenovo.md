# Deuxième essai physique du diagnostic EFI STANDARD · Lenovo

La MOA a confirmé un deuxième démarrage du diagnostic STANDARD x86_64, cette fois sur le Lenovo avec la même clé Kingston que lors du premier essai MSI. L’UEFI a lancé le diagnostic avec Secure Boot inactif. L’inventaire s’est terminé puis la MOA a confirmé le retour à Windows.

| Observation communiquée par la MOA | Valeur |
|---|---|
| Signature CPU | `000806c1` |
| RAM déclarée par UEFI | 24 219 Mio |
| Processeurs actifs déclarés dans MADT | 8 |
| Tables ACPI | 28 |
| Fonctions PCIe | 21 |
| Attribution VT-d | Indéterminée, motif `Table` |
| Activations de pilotes | 0 |

Ce relevé physique porte sur l’EFI d’inventaire. Les E/S internes et l’isolation DMA sur le Lenovo restent sans mesure physique. La date de l’essai n’a pas été communiquée. L’exécution du noyau 6A sur le Lenovo reste à qualifier. [Premier essai EFI sur le MSI](diagnostic-uefi-5y.md) · [Registre des qualifications](validation/qualifications-demarrage.md)
