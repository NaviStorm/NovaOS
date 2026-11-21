# NovaOS v0.24.0 — Diagnostic préalable aux deux VM

Référence examinée : `40a992311769866460e427a6bc10ff27f87e250a`.
Branche : `codex/novaauth-autorite-0240`. Relevé du 18 novembre 2026.

## Résultat et portée

**Qualification à deux VM non acquise.** Le diagnostic invité x86_64/P9-512
échoue avant émission du premier ticket EXEC_TICKET. Aucune admission distante
n'est enregistrée. Les trois autres profils, la suspension d'une ancienne VM
avec ticket vivant et les courses demandées restent à exécuter après résolution.
Aucune fusion, aucun tag, aucune activation ordinaire.

## Quatre passages conservés

| Passage x86_64/P9-512 | Résultat observé | Portée |
| --- | --- | --- |
| 1 | chemin de l'adaptateur natif absent ; pas d'amorçage | erreur de préparation du banc |
| 2 | ISO en mode `cli` ; NovaFS termine au code 21 | mode incorrect pour ce profil ; ISO conservée |
| 3 | ISO `novaauth-sessions`, récupération et connexion du vrai shell atteintes ; attente d'invite expirée | invite déjà reçue avant le marqueur du lanceur |
| 4 | authentification de `nsh`, `true=0`, puis `cut=26` ; aucune admission | défaut de raccord préalable démontré |

Chaque passage utilise une nouvelle copie de la même image P9 synthétique.
Ces passages ne sont pas quatre répétitions identiques permettant d'estimer une
fréquence d'intermittence. Le banc ne masque aucun échec : `succes=false`.

Pour le passage 4, la trace contient :

```text
NXLOGIN route shell=5 verificateur=6 signataire=9
NXBAIL op=NSHOPEN1 tag=1 decision=14756 valide=1 echeance=73808 retour=0 fin=14756
NXR_0__FIN
cut -f1,3 /travail/donnees.tsv
NXBAIL op=NSHEXBG1 tag=5 decision=18480 valide=1 echeance=73808 retour=26 fin=18508
NXR_26__FIN
```

Le refus précède l'échéance du bail ; il ne démontre ni une expiration, ni une
révocation distante. La table `exec_admissions` est vide. Aucun enfant CUT n'a
été admis par l'autorité. Les journaux n'établissent pas encore la restitution
complète des ressources d'un lancement EXEC, puisqu'il n'a pas commencé.

## Cause dans les interfaces réelles

`service/src/bin/common/shell_verifier.rs`, opération `NSHEXBG1`, envoie
`NXPKBEGX` puis `NXPKNAME` à NovaFS. Dans
`novaOS/user/novafs/files/package_lab.rs`, ce chemin appelle encore
`self.dev.identity_authority()` et ouvre `Registry` avec `Decision(Ledger)`.

`novaOS/user/novafs/device/authority.rs` n'accorde cette vue que pour le profil
privé et une taille de **512 + 12 = 524 pages**. Le transport indépendant du
profil présent expose **512 pages P9 + 2 pages volatiles = 514 pages**.
Le refus `Error::Unavailable` devient `w::IO`, statut natif 26, propagé au shell.
Il intervient dès `NXPKBEGX`, avant toute lecture du paquet et avant SPAWN.

Le registre isolé, sa vue authentifiée et sa récupération fonctionnent dans ce
passage. Leur raccord à l'admission distante ne remplace pas automatiquement
l'ancien chemin de résolution des paquets. Rebrancher un ancien disque d'autorité
locale ou accepter une tête restaurée comme fraîche contournerait le mandat.
Aucun tel contournement n'a été ajouté.

## Raccord proposé pour arbitrage

Limiter le changement au profil QEMU d'admission EXEC et aux installations
préexistantes, en lecture seule :

1. Le registre isolé vérifie le titulaire, la session, l'installation et les
   droits à partir de son triplet vérifié contre la tête indépendante.
2. La décision privée lie l'instance du shell, la session, l'installation,
   l'architecture, l'objet ELF, sa révision, son empreinte et l'opération.
3. NovaFS reste effecteur de lecture des octets désignés et vérifie les protections
   natives ; il ne fabrique aucune décision d'autorité et ne choisit pas la tête.
4. Le lanceur atteste les octets effectivement chargés. La barrière d'admission
   distante demeure obligatoire avant toute création, y compris après une
   décision de résolution encore valide localement.
5. Aucun nouvel accès général du registre au volume, droit d'installation,
   sélecteur public, changement P9 ou changement du noyau ordinaire.
6. Réutiliser les routes privées existantes seulement après vérification du
   budget par acteur ; aucun dépassement des quatre racines n'est permis.

Cette proposition n'est pas implémentée. L'encodage et le routage exacts doivent
respecter l'arbitrage, les droits minimaux et le plafond IPC. Les fonctions
`View::resolve_verified_factor` et `View::folder` actuellement disponibles ne
constituent pas encore un contrat de résolution d'installation.

## Banc et vérifications réalisées

Le nouvel exemple `exec_admission_volume` prépare hors ligne un volume
synthétique, l'identité Alice, les paquets CUT/CAT attestés et leurs droits de
lecture. Les quatre images de genèse ont été produites. Cette préparation
n'est ni une authentification invitée ni une preuve de lancement.

`exec_deux_vm.py --diagnostic` est volontairement limité au diagnostic d'une VM.
Il ne doit pas être présenté comme le scénario à deux VM achevé. Le contrôleur
administratif hôte signe les actions de route sous le couple serveur/domaine
provisionné ; il n'est pas accessible par le transport TLS invité.

- 83 tests hôtes existants réussis ; six avertissements Python de connexions
  SQLite non fermées restent visibles dans le journal, sans dissimulation.
- 3 tests hôtes du contrôleur administratif réussis : configuration requise,
  signature liée et journalisée, refus des substitutions et formes invalides.
- Exemple Rust compilé ; modules Python analysés syntaxiquement.
- Noyau QEMU x86_64 réutilisé sans modification de la référence qualifiée pour
  compilation ; la nouvelle ISO change uniquement le mode d'amorçage du banc.
- Aucun rejeu ABI/ELF historique ou recette quatre profils revendiqué ici : ces
  contrôles restent requis après un raccord fonctionnel valide.

Les mesures avant/après récupération du passage 4 sont inchangées pour les
acteurs mesurés : signataire `(racines=4, handles=6, attentes=0, pages=66,
  pile=64, cadres IPC globaux=31)` ; registre `(4,7,0,128,64,31)`.
Les pics observés par PID ne dépassent pas quatre racines. Ces sondes ne prouvent
pas une absence globale de fuite du futur scénario à deux VM. La réserve
instrumentée AArch64 de 301 cadres reste ouverte et inchangée.

## Références et conservation

- ISO du passage 4 : `0c184c144006fd05d981c6da9e5f9ef740f3d8402c99c194b3351fc59a561df2`.
- Noyau exact : `68e671577b5cb80dcb6c6fc6fe3875734fb4f8c47a6a6d0bfda36fff113abc75`.
- Registre inclus : `674da944fcdd8d31999435d4b630a93e3451463492d2a4ac07df8b6013b258cf`.
- P9 initial : `7c43b978ec60a531266259b759dfb5aff20cf4b5dd59b5c47a815cdea093ac3f`.

Les traces, résultats négatifs, disques de test et états d'autorité sont conservés
hors du worktree dans `OS-worktrees/preuves-deux-vm-40a9923/`. Les secrets de
laboratoire et artefacts exacts restent dans les répertoires privés dédiés et
ne sont pas commités. Le manifeste de l'archive documentaire identifie les
journaux inclus ; l'inventaire externe identifie les copies durables complémentaires.

## Suite requise

Après arbitrage et raccord validé : premier ticket réel, suspension A, B sur
copie restaurée, retrait en cours, preuve de chaque issue, retrait définitif,
activation B, refus A non expiré, nouveau ticket B. Puis toutes les courses,
pannes et régressions du mandat sur les quatre profils. Une issue inconnue
maintient `RETRAIT_EN_COURS` et B inactive ; aucun délai ou silence ne vaut
annulation. Le présent diagnostic ne lève aucune réserve de qualification.

Archive : [journaux et manifeste](validation/novaauth-deux-vm-diagnostic-v0.24.0.tar.gz), 30 fichiers contrôlés après décompression.
SHA-256 : `6b1576f4de9ab3b3ac5ff03aa54702c237997988a5db6a9e865f21c233c7b734`.
