# NovaOS v0.25.0 — Phase 3D — critères du diagnostic Ethernet

Une seule cellule est étudiée sur Q35 PIO, en écho avec des lots de huit trames de 1 514 octets. Les mesures réemploient le pair, le modèle QEMU, le firmware et les compteurs de phase 3C. Le débit est calculé sur la fenêtre invitée. Le p99 couvre l'aller-retour du lot.

L'expérience déplace uniquement le traitement de la fermeture dans une fonction froide que le compilateur ne peut incorporer. L'image témoin est reconstruite à partir de la même révision de phase 3C, avec la même chaîne et les mêmes options. Chaque variante est identifiée par son ELF. Les images 3B et 3C archivées donnent deux points de référence supplémentaires.

Effectuer cinq démarrages de chaque variante, en alternant l'ordre témoin/essai et essai/témoin. Retenir une correction seulement si le gain de débit et la baisse du p99 sont observés dans les cinq paires, si les médianes retrouvent le témoin 3B à 1 % près ou le dépassent, et si les IPC par trame n'augmentent pas de plus de 1 %. Un effet de disposition binaire peut expliquer une sensibilité au placement sans établir quel bloc traduit par QEMU est responsable. Les tests de fermeture partielle, de révocation et de propriété restent nécessaires avant toute intégration d'une correction.
