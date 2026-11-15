# Phase 4 — Prévention et notification des fautes sur Q35

## Résultat et cause

Le contrôleur EDU et l'unité VT-d de QEMU 11.1.3 refusent l'écriture de 4 Kio sur la page en lecture seule dans les deux états de cache testés. Le noyau compare la totalité des deux pages avec deux motifs distincts. À chaud, l'unité conserve un FRCD invalide, son compteur d'IRQ reste constant et le masque d'interruption est ouvert avant et après la commande. À froid, le FRCD désigne le SID 48 et l'adresse attendue, le BSP reçoit une MSI de faute et le superviseur acquitte l'enregistrement. [Banc et oracle](../tests/vtd_edu.py) · [Sondes du superviseur](../kernel/src/sched/process/vtd.rs)

La différence vient des deux couches du modèle QEMU. À froid, le remappeur contrôle les droits en parcourant les tables, enregistre la faute primaire et demande la MSI. À chaud, il rend les droits de la traduction mémorisée. Le gestionnaire d'espace d'adressage applique encore ces droits et écarte la transaction avant tout accès à la page cible. Ce second contrôle n'alimente pas le registre d'enregistrement du remappeur. Le code du tag officiel a été vérifié pour la version de l'exécutable du banc. [Traduction, parcours et enregistrement VT-d dans QEMU v11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/hw/i386/intel_iommu.c) · [Contrôle des permissions en aval dans QEMU v11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/system/physmem.c)

La documentation QEMU décrit l'émulation Q35, les options de cache et les registres EDU. Elle ne promet pas une notification distincte pour chaque refus de permission. Les sources expliquent aussi un autre phénomène observé à froid. L'unité peut regrouper les signalements successifs du même émetteur tant qu'un enregistrement est en attente. Sur une commande de 4 Kio, QEMU trace plusieurs refus dans la même page, mais dépose un seul FRCD et émet une seule MSI. À chaud, le FRCD est déjà libre, le masque ouvert et la trace ne contient aucun événement primaire de faute pour l'intervalle. [Options du modèle Intel](https://www.qemu.org/docs/master/system/qemu-manpage.html) · [Contrôleur EDU](https://www.qemu.org/docs/master/specs/edu.html) · [Gestion des fautes primaires](https://github.com/qemu/qemu/blob/v11.1.3/hw/i386/intel_iommu.c#L576-L645)

## Oracle de notification

Le superviseur relève FSTS, les deux mots de FRCD, FECTL, le compteur d'IRQ et le CPU destinataire, IQH, IQT et le reçu WAIT. Les relevés ont lieu avant soumission, après retour de RUN à zéro et, lorsqu'une faute est signalée, de part et d'autre de l'acquittement. Les champs de FRCD peuvent conserver un ancien SID, une adresse ou une raison quand son bit F est nul. L'oracle exige F avant d'interpréter ces valeurs. Les relevés sont passifs et séquentiels. Un handler peut s'intercaler avant l'échantillonnage de fin de commande. Le masque et la livraison sont vérifiés au point de collecte, après réception de l'IRQ. [Instrumentation privilégiée](../kernel/src/hardware/vtd_lab.rs)

| Cas | Mémoire et traduction | Enregistrement, interruption et acquittement |
|---|---|---|
| Première demande puis répétition sur la même ancienne IOVA | Deux commandes distinctes sur le contexte retiré. Les deux anciens cadres réalloués à une sentinelle gardent tous leurs octets. Chaque demande est drainée. | La deuxième tentative intervient après acquittement de la première. Deux raisons 2, deux vecteurs 0x53 et deux acquittements. |
| Écriture RO à chaud | Deux pages intactes. Les traces QEMU montrent 1 024 consultations de la traduction RO pour cette commande. | Aucune nouvelle faute primaire, aucun nouveau MSI. Le FRCD conserve F=0. |
| Écriture RO après invalidation | Deux pages intactes. Le reçu WAIT délimite l'invalidation terminée. QEMU recharge le contexte. | Raison 5, un FRCD, une MSI, acquittement et réarmement vérifiés. |
| Lecture WO, autre page et autre génération | Lecture refusée par VT-d, tampon EDU dépourvu d'autorisation d'export après cette tentative. | Raison 6, un FRCD et une MSI. Un bit RUN éteint ne constitue pas une preuve du contenu lu par l'EDU. |
| IOVA absente puis page privée étrangère | Aucune modification des pages contrôlées. Le second processus vérifie les 4 096 octets de sa propre page. | Un enregistrement par commande, raison 5 avec son adresse propre. |
| Recréation du domaine | DID 1 réutilisé avec une nouvelle époque, une nouvelle plage IOVA et deux pages attribuées. Les anciens droits sont refusés. | Chaque nouveau lot repart de FSTS=0 avec le masque ouvert, y compris après un FRCD déjà acquitté. |

Avant chaque acquittement observé, FSTS vaut 2, F vaut 1 et le handler a masqué la source sur le BSP. Après l'écriture W1C du bit F et le réarmement, FSTS, F et FECTL valent zéro. Les six FRCD du profil standard donnent six compteurs d'interruption successifs. L'oracle associe chaque série d'événements hôte au reçu matériel WAIT de la même commande. Il examine séparément la consultation d'une traduction, l'intégrité des pages invitées, la livraison de la MSI et la libération du FRCD. [Contrôle des séquences](../tests/vtd_edu.py)

## Redémarrage du périphérique

Dans le profil mono, après la campagne et la fermeture de la carte, l'hôte arrête les vCPU, conserve l'archive du premier démarrage et demande un véritable `system_reset` par QMP dans le même processus QEMU. Le firmware rouvre PCI_COMMAND à 7. Le second noyau observe RUN à zéro mais retrouve une longueur DMA de 4 096 octets et la direction d'écriture de la commande précédente. Il refuse cette carte avant toute écriture PCI. QMP relit RTADDR et GSTS nuls. Aucun nouveau contexte ni demande Ring 3 n'est créé. Le cas donne un résultat après reset système sans exposer une page au périphérique hérité. Les campagnes sur des VM neuves créent, elles, un nouvel objet EDU vierge. L'EDU est un périphérique PCI conventionnel et ne présente pas de mécanisme FLR PCIe individuel qualifié. [Modèle EDU 11.1.3](https://github.com/qemu/qemu/blob/v11.1.3/hw/misc/edu.c#L355-L425) · [Réinitialisation PCI de QEMU](https://github.com/qemu/qemu/blob/v11.1.3/hw/pci/pci.c#L526-L580)

## Relecture et limites

Le JSON produit par le banc contient les journaux série, les traces QEMU compressées, leurs empreintes et les registres finaux relus par QMP. La commande suivante rejoue uniquement l'analyse d'une archive locale passée en argument. Elle n'émet aucune opération DMA.

```sh
cd novaOS
PYTHONPATH=tests python3 -B - docs/validation/materiel-phase4-vtd.json <<'PY'
import base64, gzip, hashlib, json, sys
from pathlib import Path
from vtd_edu import validate
for row in json.loads(Path(sys.argv[1]).read_bytes()):
    if row["profil"] == "normal":
        serial = row["serie"]
        trace = gzip.decompress(base64.b64decode(row["journal_qemu_gzip_base64"]))
        assert hashlib.sha256(trace).hexdigest() == row["sha256_qemu"]
        assert hashlib.sha256(serial.encode()).hexdigest() == row["sha256_serie"]
        result = validate(serial, trace.decode(), "normal", row["vcpus"])
        assert json.loads(json.dumps(result)) == row["resultats"]
        print(row["vcpus"], result["diagnostic"])
PY
```

Le signalement résulte du modèle QEMU, du type exact d'accès et de l'état de ses caches. Le sous-ensemble testé utilise les enregistrements primaires legacy. Les assertions établissent les droits de ce domaine et de ces pages de test. Elles ne mesurent pas une confidentialité universelle, le comportement d'un Intel physique ni le nombre d'interruptions qu'un autre contrôleur produirait. Le noyau ferme l'admission sur défaut reconnu, conserve les confirmations séparées du contrôleur et de l'IOMMU et n'utilise jamais le silence des IRQ comme autorisation d'accès.
