# Performances des ELF de recette

Médiane de trois lancements après un passage de contrôle. Le temps entre l’envoi de la commande et le retour du prompt inclut le lancement, les E/S P9 et la transmission du résultat sur la console de recette. Le débit compte tous les octets des fichiers des deux opérandes. Le pic mesure les pages de tas du processus attribuées par HEAP_RESIZE jusqu’à la fin de la comparaison. La pile, le code, le noyau et QEMU sont exclus.

| ISA et secteur P9 | Charge | Entrée o | Diff o | ms | Mio/s | Pic tas Kio |
|---|---|---:|---:|---:|---:|---:|
| aarch64-512 | equal | 4352 | 0 | 57.62 | 0.0720 | 64 |
| aarch64-512 | start | 4356 | 50 | 60.57 | 0.0686 | 64 |
| aarch64-512 | end | 4356 | 54 | 64.07 | 0.0648 | 64 |
| aarch64-512 | many | 4384 | 418 | 63.85 | 0.0655 | 64 |
| aarch64-512 | tree | 4370 | 80 | 73.81 | 0.0565 | 64 |
| aarch64-4096 | equal | 4352 | 0 | 61.15 | 0.0679 | 64 |
| aarch64-4096 | start | 4356 | 50 | 61.28 | 0.0678 | 64 |
| aarch64-4096 | end | 4356 | 54 | 61.18 | 0.0679 | 64 |
| aarch64-4096 | many | 4384 | 418 | 63.46 | 0.0659 | 64 |
| aarch64-4096 | tree | 4370 | 80 | 74.51 | 0.0559 | 64 |
| x86_64-512 | equal | 4352 | 0 | 48.97 | 0.0848 | 64 |
| x86_64-512 | start | 4356 | 50 | 55.10 | 0.0754 | 64 |
| x86_64-512 | end | 4356 | 54 | 54.54 | 0.0762 | 64 |
| x86_64-512 | many | 4384 | 418 | 57.03 | 0.0733 | 64 |
| x86_64-512 | tree | 4370 | 80 | 67.01 | 0.0622 | 64 |
| x86_64-4096 | equal | 4352 | 0 | 49.28 | 0.0842 | 64 |
| x86_64-4096 | start | 4356 | 50 | 54.12 | 0.0768 | 64 |
| x86_64-4096 | end | 4356 | 54 | 54.11 | 0.0768 | 64 |
| x86_64-4096 | many | 4384 | 418 | 57.08 | 0.0732 | 64 |
| x86_64-4096 | tree | 4370 | 80 | 69.08 | 0.0603 | 64 |
