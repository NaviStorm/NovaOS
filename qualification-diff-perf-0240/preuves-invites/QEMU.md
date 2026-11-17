# Performances des ELF de recette

Médiane de trois lancements après un passage de contrôle. Le temps entre l’envoi de la commande et le retour du prompt inclut le lancement, les E/S P9 et la transmission du résultat sur la console de recette. Le débit compte tous les octets des fichiers des deux opérandes. Le pic mesure les pages de tas du processus attribuées par HEAP_RESIZE jusqu’à la fin de la comparaison. La pile, le code, le noyau et QEMU sont exclus.

| ISA et secteur P9 | Charge | Entrée o | Diff o | ms | Mio/s | Pic tas Kio |
|---|---|---:|---:|---:|---:|---:|
| aarch64-512 | equal | 30240 | 0 | 147.55 | 0.1954 | 64 |
| aarch64-512 | start | 30244 | 100 | 164.69 | 0.1751 | 64 |
| aarch64-512 | end | 30244 | 104 | 169.79 | 0.1699 | 64 |
| aarch64-512 | many | 30272 | 826 | 151.18 | 0.1910 | 64 |
| aarch64-512 | tree | 30258 | 130 | 176.91 | 0.1631 | 64 |
| aarch64-4096 | equal | 30240 | 0 | 150.49 | 0.1916 | 64 |
| aarch64-4096 | start | 30244 | 100 | 148.82 | 0.1938 | 64 |
| aarch64-4096 | end | 30244 | 104 | 168.41 | 0.1713 | 64 |
| aarch64-4096 | many | 30272 | 826 | 160.75 | 0.1796 | 64 |
| aarch64-4096 | tree | 30258 | 130 | 181.42 | 0.1591 | 64 |
| x86_64-512 | equal | 30240 | 0 | 132.07 | 0.2184 | 64 |
| x86_64-512 | start | 30244 | 100 | 146.97 | 0.1963 | 64 |
| x86_64-512 | end | 30244 | 104 | 151.33 | 0.1906 | 64 |
| x86_64-512 | many | 30272 | 826 | 147.51 | 0.1957 | 64 |
| x86_64-512 | tree | 30258 | 130 | 145.40 | 0.1985 | 64 |
| x86_64-4096 | equal | 30240 | 0 | 124.77 | 0.2311 | 64 |
| x86_64-4096 | start | 30244 | 100 | 131.77 | 0.2189 | 64 |
| x86_64-4096 | end | 30244 | 104 | 130.62 | 0.2208 | 64 |
| x86_64-4096 | many | 30272 | 826 | 131.30 | 0.2199 | 64 |
| x86_64-4096 | tree | 30258 | 130 | 153.99 | 0.1874 | 64 |
