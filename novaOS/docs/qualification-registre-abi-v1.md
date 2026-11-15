# Qualification du registre ABI v1

Le registre recense 41 sélecteurs dispatchés sur x86_64 et AArch64. La base auditée est la fusion de la phase 1 de v0.21.0, `a6df93fb8687c68893680707f8763471d9f8a2b3`. Par rapport à la clôture v0.20.0, les modules installation et instances ont été ajoutés et six sélecteurs d’instance ont rejoint le dispatch. L’audit et la préparation documentaire non fusionnés n’ajoutaient aucun sélecteur sur leur propre ligne de développement. La v0.21.0 reste ouverte.

## Correspondance et génération

L’[index](registre-abi.md) relie numéro, constante qualifiée, traitement du noyau et, lorsqu’il existe, point d’entrée du SDK. Les trames du service fichier, du bloc et du dialogue privé d’édition utilisent leurs espaces distincts. Par exemple, `File::read` émet une requête fichier et le service bloc possède sa propre opération de lecture. Les ports noyau du pilote et du gestionnaire d’instances restent soumis aux droits et aux conditions du lanceur.

Les deux portes matérielles aboutissent au même dispatch. Les handlers, les wrappers Rust et les conventions de registres conservent leurs sources de la base auditée. Le gabarit [native.rs.in](../abi/native.rs.in) contient les dispositions Rust et les emplacements des sélecteurs. Le générateur injecte les numéros du TOML. La sortie `native.rs` est aussi une source de l’archive SDK v0.2.0. Son marquage comme fichier généré se trouve dans `abi/.gitattributes`, afin de conserver exactement ses octets publiés. L’en-tête C/C++ décrit constantes et structures pour un futur SDK.

Les quatre sondes assembleur indépendantes dans `user/{x86_64,aarch64,fp_x86_64,fp_aarch64}.S` conservent leurs appels historiques à exit, yield ou id. Les sondes `user/calls.rs` et `kernel/src/sched/process/ipc/probes/contract.rs` injectent des numéros inconnus ou une mauvaise version. Les fixtures de réservation, prêt et DMA utilisent les plages du prototype. Ces sondes gardent leur valeur de témoin indépendant. [Historique des plages](es-v0.16.6-integration-interfaces.md)

## Résultats du 15 novembre 2026

| Contrôle | AArch64 | x86_64 |
|---|---|---|
| En-tête C11 et C++11, compilation freestanding | 2/2 | 2/2 |
| Tests du SDK sur macOS | 3/3 natifs | 3/3 sous Rosetta |
| ELF v0.20.0 de l’exemple SDK, QEMU TCG, 2 vCPU | EL0, sortie 0 | Ring 3, sortie 0 |
| ELF v0.20.0 de qualification, QEMU TCG, 2 vCPU | 62 438 cadres avant et après | 62 996 cadres avant et après |
| Sonde ABI historique, noyau ordinaire, 256 Mio | Réussie | Réussie |
| Sonde instances, noyau ordinaire, 1, 2 et 4 vCPU | 3/3 profils | 3/3 profils |

Chacun des quatre ELF provient de la campagne historique. Le banc a relu et comparé son SHA-256 à la preuve publiée avant compilation du noyau, puis contrôlé la copie intégrée au catalogue. Les douze journaux série ont été archivés et relus. Les six tests du registre ont réussi. L’archive distribuée du SDK contient toujours 21 237 octets et son SHA-256 vaut `6feff113b64eef2352c430fc5c64fc2f1f865cfe40339ecbec5e6a4f3ff469a4`. [Mesures et empreintes](validation/registre-abi-v1.json) · [Journaux série](validation/registre-abi-v1.tar.gz) · [Référence des ELF v0.20.0](validation/rust-sdk-phase2-v0.20.0.json)

## Reproduire

Depuis la racine du worktree, configurer les temporaires selon `AGENTS.md`. Le générateur utilise Python 3.11 ou plus récent. Les tests historiques utilisent le dépôt Git et les deux commits de référence.

```sh
make -C novaOS abi-generate abi-check
python3 -B novaOS/tests/abi_registre.py -v
python3 distribution/scripts/sdk_rust.py --check
for arch in aarch64 x86_64; do
    make -C novaOS ARCH="$arch" kernel
    (cd novaOS/tests && NOVA_ARCH="$arch" python3 -B -c 'import os; from demarrage import abi_scenario; abi_scenario(os.environ["NOVA_ARCH"], 256)')
    NOVA_ARCH="$arch" python3 -B novaOS/tests/instances.py
done
```

Pour chaque architecture, fournir les ELF conservés dans les preuves du SDK v0.20.0. Le banc refuse un binaire dont l’empreinte diffère de celle publiée.

```sh
python3 -B novaOS/tests/abi_elf_archive.py "$arch" sdk-example "$elf_exemple"
python3 -B novaOS/tests/abi_elf_archive.py "$arch" threads-test "$elf_qualification"
```

La qualification porte sur les interfaces compilées et les invités QEMU TCG. Les longues campagnes disque, DMA et UTM relèvent des versions qualifiées de ces composants. Le registre conserve la portée des contrats existants, y compris la classification expérimentale des signatures Rust.
