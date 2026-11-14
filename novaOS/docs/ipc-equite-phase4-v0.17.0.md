# Équité de l’IPC multicœur, phase 4

Chaque essai démarre une VM QEMU TCG 11.1.3 de 256 Mio sur macOS ARM64. AArch64 utilise Cortex-A72 et GICv2, x86_64 utilise Q35 et le modèle CPU `max`. Chaque architecture est exercée à 1, 2 et 4 vCPU, deux fois par configuration. Le candidat noyau est `a8dc2534ac9fabcca2e480d1054fd0b60f180f17`. Le profil fonctionnel associe la charge utilisateur et les crochets de la sonde aux changements déjà retenus sur la branche. Les empreintes ELF des constructions `ipc-equite-lab`, les relevés individuels et le temps hôte de démarrage et d’essai sont archivés. [Données individuelles](validation/ipc-equite-phase4-v0.17.0.json)

## Contrat du banc

Le contrôleur lance quatre couples client/serveur dans des espaces utilisateurs distincts, avec de vrais canaux aller et retour. Chaque acteur contrôle le rang suivant, la longueur et les 128 octets de chaque message. Chaque client annonce la fin par une notification privée après sa dernière réponse. Les deux partenaires doivent avoir effectué exactement le même nombre de tours à la fermeture. Deux couples ont initialement tous les CPU admissibles. Les deux autres sont épinglés, avec partage du CPU en mono-vCPU. Sur les configurations SMP, le premier couple est successivement épinglé à CPU0 et CPU1 et doit s’être exécuté sur les deux CPU. Ces changements de masque appartiennent au seul contrôleur du banc.

Pendant les échanges, un processus calcule sans céder le CPU et un autre tente de publier dans une file de huit messages maintenue pleine. Chaque refus `WOULD_BLOCK` du producteur est compté. Après l’épreuve, les huit messages initiaux sont retirés dans l’ordre, avec vérification de leurs données. Deux autres processus sont bloqués en réception. Leur propriétaire révoque les deux délégations dans une seule transaction. Les deux appels doivent rendre `REVOKED`. Les trois processus sont ensuite terminés et collectés pendant que les dix autres acteurs continuent. Le tas et le nombre de cadres libres doivent retrouver leurs valeurs initiales après collecte générale.

La première période de progression commence après la collecte des trois processus temporaires. Le contrôleur s’endort pendant 250 ticks avant de relever les compteurs de chaque acteur, trois fois. Un acteur soutenu sans progrès pendant une période fait échouer le test. Une échéance hôte de 120 secondes par VM borne également le cas où le contrôleur cesserait lui-même de s’exécuter. Le réveil de deux receveurs est simultané au sens de la publication des issues sous la même garde. Les reprises sur CPU sont mesurées indépendamment.

Pour chaque génération de TCB observée, le banc enregistre les entrées en exécution depuis un autre thread, le maximum entre deux entrées, les délais blocage→réveil et prêt→exécution, les préemptions, les CPU effectivement utilisés et les attentes READY excédant une seconde. Le délai prêt→exécution inclut aussi la remise en file après préemption ou cession. Une seconde est un seuil d’alerte du banc. Les horloges sont celles du temps écoulé dans l’invité et peuvent inclure sa désélection par macOS. Une première attente commencée avant l’activation de la sonde est mesurée à partir de cette activation. Le maximum entre deux entrées vaut zéro pour une génération qui n’a eu qu’une reprise.

Le noyau possède deux classes, normale et idle. Dans la construction d’équité, toute décision idle vérifie l’absence d’un prêt normal admissible, avec l’exception historique du BSP choisi pour collecter les threads terminés. Les données enregistrées concernent les treize acteurs de charge. Les threads idle et le contrôleur sont le mécanisme de service du banc. La mesure de progression applicative reste distincte du nombre de passages en exécution, car des passages d’ordonnancement sans réponse IPC ne suffisent pas à établir la progression.

## Résultats

Les maxima sont pris sur les treize acteurs et sur les deux démarrages de chaque cellule. Les plages de progression indiquent les valeurs minimale et maximale parmi les quatre clients sur les trois périodes de chacun des deux démarrages. La progression du calcul se compte en blocs de 1 024 opérations, celle de la file pleine en refus vérifiés et celle des couples en tours complets. Pour les trois acteurs temporaires, la progression compte les deux retraits acquittés par le propriétaire et le retour terminal reçu par chacun des deux destinataires.

| Architecture | vCPU | Tours par client et par période | Départs en exécution, deux essais | Maximum entre départs, ms | Maximum bloqué→réveillé, ms | Maximum prêt→élu, ms | Alertes READY > 1 s |
|---|---:|---:|---:|---:|---:|---:|---:|
| aarch64 | 1 | 117–125 | 9007 | 94,784 | 64,005 | 63,923 | 0 |
| aarch64 | 2 | 1829–4865 | 185681 | 55,675 | 55,177 | 53,237 | 0 |
| aarch64 | 4 | 1890–5283 | 171208 | 119,672 | 105,567 | 98,514 | 0 |
| x86_64 | 1 | 109–125 | 8939 | 49,046 | 72,657 | 40,325 | 0 |
| x86_64 | 2 | 1362–4743 | 173066 | 105,176 | 104,864 | 102,849 | 0 |
| x86_64 | 4 | 1550–3663 | 135375 | 345,091 | 344,921 | 182,995 | 0 |

Les douze essais ont validé les rangs, le contenu et l’acquittement de chaque échange, la progression de tous les acteurs éligibles, la fin des deux récepteurs révoqués et la restitution des cadres. L’occupation finale des pages IPC est nulle dans les douze invités. Les sondes et la table bornée par le nombre maximal de TCB sont compilées uniquement avec `ipc-equite-lab`. Elles ne participent pas au banc de débit.

## Portée de la non-famine

Dans une exécution où le CPU admissible revient régulièrement à la sélection, la FIFO choisit le plus ancien prêt compatible et replace les nouveaux prêts à sa suite. La garde interprocesseurs utilise une acquisition par comparaison-échange. Un acquéreur peut arriver devant un concurrent qui attendait déjà. La politique ne fournit donc pas de borne formelle de service pour l’acquisition de la garde. Même si chaque section critique se termine, le réessai après libération pourrait perdre indéfiniment. La progression observée ici vaut pour les charges, durées et topologies testées. Pour obtenir une borne sur le nombre de dépassements, une piste serait une admission FIFO des acquéreurs extérieurs, en maintenant les réentrées locales et la relève atomique de garde au changement de pile. Il faudrait vérifier la progression des invalidations TLB et des arrêts pendant la file, les cas de CPU suspendu et le coût du convoi avant d’adopter cette politique. Une borne en temps réel exigerait aussi des hypothèses sur l’ordonnancement hôte.

Depuis `novaOS/`, le banc se reproduit ainsi. Chaque option `--arch` et `--cpus` peut sélectionner une seule cellule. [Lanceur](../tests/ipc_equite.py) · [Sondes de l’ordonnanceur](../kernel/src/sched/equite.rs) · [Charge dans les processus utilisateur](../user/ipc_equite.rs)

```sh
export RUSTC="$(rustup which --toolchain stable rustc)"
rustup run stable cargo build --release --offline --locked --features ipc-equite-lab \
    --target aarch64-unknown-none-softfloat --target x86_64-unknown-none \
    --target-dir build/equite/elf
python3 -B tests/ipc_equite.py build/equite/elf build/equite/releve1
python3 -B tests/ipc_equite.py build/equite/elf build/equite/releve2 --arch x86_64 aarch64 --cpus 4 2 1
```
