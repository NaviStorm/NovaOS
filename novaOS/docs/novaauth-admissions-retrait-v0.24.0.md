# NovaOS v0.24.0 — Admissions EXEC_TICKET et états du retrait

## Portée et référence

Implémentation en cours sur `codex/novaauth-autorite-0240`, depuis
`fa046070fae890ce397eb53827352e7a62c9372a`. Le raccord est réservé à
l'export explicite `--admission-exec` du profil QEMU. Ce document ne prononce
aucune qualification à deux VM ni aucune autorisation de fusion.

La consommation du ticket est distincte de l'admission durable distante et de
la publication locale de l'enfant. Le nombre de sélecteurs ABI reste 48. Les
anciens appels SPAWN et le noyau ordinaire ne reçoivent pas ce raccord.

## États obligatoires

| État durable de A | Nouvelles admissions A | Admissions antérieures | Activation B |
| --- | --- | --- | --- |
| `ACTIVE` | Possibles après tous les contrôles | Réconciliables par identifiant | Interdite |
| `RETRAIT_EN_COURS` | Interdites | Création achevée avec enfant identifié, ou annulation irréversible à démontrer | Interdite |
| `RETIREE` | Interdites | Toutes terminales, aucune ne peut encore créer un processus | Possible après approbation indépendante et contrôles de récupération |

Le premier accusé constate uniquement `RETRAIT_EN_COURS`. Le retrait définitif
n'est acquitté qu'après enregistrement durable de `RETIREE`. L'approbation de
B vérifie cet état dans la même transaction que le remplacement de la route.
Elle ne valide jamais une ancienne génération P9.

Une réponse perdue, un délai écoulé, la fin du bail ou l'injoignabilité de A ne
constituent pas une annulation. Si l'issue d'un seul lancement demeure inconnue,
A reste `RETRAIT_EN_COURS` et B inactive, sans délai déclenchant une promotion.
La réserve doit être présentée à la MOA/MOE avec les identifiants concernés.

## Points de décision

1. **Admission** : commit synchronisé de l'opération unique dans l'autorité.
   La vérification de `ACTIVE`, l'unicité du ticket et l'enregistrement sont
   effectués dans la même transaction. Un reçu ne sort qu'après ce commit.
2. **Début du retrait** : commit synchronisé interdisant les nouvelles
   admissions. Son ordre relatif aux admissions concurrentes est explicite.
3. **Issue locale** : sous le verrou du noyau, passage unique à la construction,
   puis publication de l'enfant avec son identité complète, ou annulation
   empêchant définitivement toute construction ultérieure. Une construction en
   cours ne peut pas être annulée par simple expiration. Son nettoyage doit
   précéder l'attestation d'annulation.
4. **Issue distante** : l'autorité vérifie l'attestation transmise par le registre
   et le signataire isolés. Elle enregistre l'identité de l'enfant ou l'annulation.
   « Lancement achevé » signifie ici création publiée, pas fin d'exécution de
   l'enfant. Aucune nouvelle création ne reste possible pour cette opération.
5. **Fin du drainage** : transaction vérifiant toutes les issues, puis publication
   de `RETIREE`. Une identité enfant absente ou une issue non ordonnée bloque cette
   transition. Une consultation du reçu est sans nouvelle mutation.

Le noyau réserve la consommation avant l'attente distante : deux threads ne
peuvent pas construire deux enfants. La réponse d'admission est liée au serveur,
au domaine, à l'incarnation, à l'instance, à la session, à l'installation, à
l'empreinte ELF et au ticket. Les refus et droits historiques restent applicables.

## Réponses perdues et annulation

Une demande envoyée n'est jamais répétée implicitement. Les consultations portent
sur son identifiant et un défi frais. Une réponse « inconnue » ne prouve pas que
la demande n'est pas encore en transit. Si le noyau a irréversiblement annulé la
construction, l'autorité peut enregistrer une tombe d'annulation empêchant cette
admission retardée. Cette tombe exige l'attestation locale ; elle ne découle pas
d'un délai ou d'une absence de réponse.

Si le registre perd son suivi local sans pouvoir retrouver cette preuve, le
raccord doit conserver l'issue incertaine. Aucune récupération depuis les disques
restaurés ne doit fabriquer une attestation de non-création de l'ancienne VM.

## Interfaces privées et bornes

L'enveloppe canonique d'admission compte 256 octets. Elle est une donnée
applicative, distincte des métadonnées du transport hors P9. Les opérations privées
40/41/42 servent respectivement à l'admission, à son issue et à sa consultation,
sur la conversation registre/signataire déjà existante. Aucune commande
administrative de retrait n'est exposée sur cette route.

Le registre reçoit une feuille SEND/RECEIVE du ticket existant, liée à son instance
supervisée. Le noyau produit les métadonnées et l'issue sur cette route privée.
Aucune racine supplémentaire n'est créée par ce raccord. Les mesures effectives
des handles, des cadres et des pics de racines restent à établir dans les invités.
Le registre doit rester à quatre racines au maximum.

L'autorité borne à 64 les admissions simultanément non résolues par incarnation.
La saturation est atomique : aucun événement d'admission supplémentaire n'est
publié. Les tombes terminales et les reçus restent des données durables d'audit.

## Qualification requise et état réel

Les tests hôtes vérifient l'ordre durable, les courses admission/retrait, la
consommation concurrente, la perte des réponses, le refus d'activer B pendant le
drainage, la persistance après redémarrage de l'autorité, les tombes d'annulation,
les liaisons des signatures, les tailles et la saturation. Ils ne prouvent pas
l'application de cette barrière par le VFS ou le lanceur de l'invité.

Les tests Rust de la machine locale vérifient l'annulation irréversible, la
construction en cours, la publication unique et le rapprochement des issues.
Le raccord et les services compilent sur AArch64 et x86_64 ;
une compilation n'est pas une preuve de comportement invité.

Restent obligatoires, avant toute conclusion de qualification :

- Deux VM isolées partageant une autorité unique durable ; A conserve son vrai
  ticket non consommé, B restaure ses disques et effectue la récupération 2-sur-2.
- Admissions avant, pendant et après retrait ; création retardée après admission ;
  réponses perdues ; autorité inaccessible ; consommation concurrente ; activation
  B refusée pendant le drainage ; nouveau ticket B ; rejeu après redémarrage A.
- Refus par retrait distingué du refus par expiration, PID et absence de création
  interdite, restitution des ressources, mesures IPC et mémoire.
- AArch64/P9-512, AArch64/P9-4096, x86_64/P9-512 et x86_64/P9-4096.
- Recettes VFS avant révocation, après restauration et après reprise ; ABI v1,
  quatre ELF historiques, IPC, NovaAuth et conservation du format P9.

La réserve mémoire instrumentée AArch64 de 301 cadres reste ouverte. Les preuves
antérieures restent conservées ; aucune distribution ordinaire, UTM ou machine
physique n'est déclarée qualifiée par ce document.

## Jalon de vérification du 18 novembre 2026

83 tests hôtes réussis, dont 12 sur les admissions et le retrait ; 5 tests de la
machine locale et 3 tests cryptographiques par architecture hôte (ARM64 Darwin
et x86_64 sous Rosetta) ; 9 contrôles ABI réussis ; les deux noyaux QEMU compilent.
Aucun nouvel essai QEMU de bout en bout n’est compté dans ce jalon.

[Archive des journaux et résultats](validation/novaauth-admissions-retrait-jalon-v0.24.0.tar.gz),
SHA-256 `2d3c02c8e31abba63ce5760d584c69744cdc3299c26f845b825cd099a6d47b98`.
Son manifeste a été vérifié par relecture des membres décompressés. Les binaires
contenant les clés de laboratoire restent hors Git, dans
`OS-worktrees/artefacts-prives-admission-exec-fa04607/` ; leurs copies ont été
relues et comparées aux originaux. Les premiers journaux de compilation ayant
échoué sur une cible absente et le premier essai hôte sans `NOVA_DETENTEUR` restent
archivés avec les reprises réussies.
