# Chargeur AArch64 du profil QEMU de lancement

Le profil `profile.py --lancement` prépare un export jetable. Il y remplace la copie AArch64 de Limine dans l’ESP et à côté du relais BootID. L’image officielle et ses empreintes restent dans `novaOS/third_party/limine`. Le binaire du profil provient de Limine 10.8.5, sources `5a51c7bdb0b8711be9b2ea39f5180375c15157b2`, avec le correctif local `carte-complete.patch`. La licence du chargeur accompagne les ISO. [Publication amont](https://github.com/Limine-Bootloader/Limine/releases/tag/v10.8.5) · [Conditions de redistribution](../../../../licenses/limine-bootloader.txt)

Dans `build_pagemap`, l’allocation de la copie peut scinder une région utilisable. Le chargeur réserve la capacité maximale déjà admise par la réponse Limine, lit ensuite le compteur courant, le contrôle et copie la carte sans allocation intermédiaire. Le tampon de travail appartient au chargeur jusqu’à la restitution normale de ses cadres. Les protections des pages sont celles du chargeur et du noyau habituels.

## Reconstruire et comparer

Travailler dans un sous-répertoire neuf de `NOVAOS_TMPDIR` conformément à `AGENTS.md`. Fournir le chemin absolu du présent dossier dans `LAB`. La reconstruction qualifiée utilise Clang/LLVM 23.1.5, LLD 23.1.1, GNU make 3.81, GNU sed et GNU awk. Mettre le répertoire LLVM et GNU sed en tête de `PATH` et indiquer le chemin absolu de LLD dans `LD_FOR_TARGET`. Le PE est produit par `llvm-objcopy` depuis l’ELF du chargeur.

```sh
set -eu
work=$(mktemp -d "$NOVAOS_TMPDIR/limine-qualification-XXXXXXXX")
cd "$work"
curl -fL -o limine-10.8.5.tar.xz https://github.com/Limine-Bootloader/Limine/releases/download/v10.8.5/limine-10.8.5.tar.xz
printf '%s  %s\n' 84e1e2789a0be980f791627da0e9b189b9ae95cf629acd1994992ba3dc275c5d limine-10.8.5.tar.xz | shasum -a 256 -c -
tar -xf limine-10.8.5.tar.xz
(cd limine-10.8.5 && patch -p1 < "$LAB/carte-complete.patch")
mkdir build
cd build
../limine-10.8.5/configure --enable-uefi-aarch64
make limine-uefi-aarch64
cmp bin/BOOTAA64.EFI "$LAB/BOOTAA64.EFI"
```

L’empreinte attendue du PE est `7b9cae50fc4649db9fb1ed48102c2b90d5df0ae2ad1113149d1b7f8707349b7e`. L’ESP source, son PE et celui du laboratoire sont contrôlés avant la construction de l’export. Une reconstruction différente demande une qualification avant d’actualiser ces empreintes.
