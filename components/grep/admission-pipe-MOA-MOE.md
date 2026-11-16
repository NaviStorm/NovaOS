# ARC-027 · Demande de décision MOA/MOE sur le lancement de grep

**Décision sollicitée.** Autoriser, dans un lot distinct, l’admission d’un ELF utilisateur identifié comme `grep` et son rôle de lecteur pour `cat fichier | grep motif`. La présente branche apporte la commande macOS, le cœur partagé et des essais documentaires. L’arbitrage porte sur le lancement et la délégation de capacités dans NovaOS.

Sur la base `816843fa6f5a8ae35f64ef0c3f96a67652bbc384`, `spawn_program` traite `PIPE_CAT`, `PIPE_WC` et `PIPE_HEAD`, puis contrôle l’image et le droit d’attente du parent. Le rôle et l’image sont liés. Le catalogue d’ELF embarqués et le choix du lecteur dans `nsh` doivent évoluer ensemble. Le parcours sur fichier doit également admettre l’image et déléguer la lecture des seuls opérandes sélectionnés. [Lanceur](../../novaOS/kernel/src/sched/process/ipc/spawn.rs) · [Exécuteur du shell](../shell/executor.rs) · [Contrat de flux](../../novaOS/docs/contrat-flux-arc027-v0.23.x.md)

Pour la première tranche, octroyer au lecteur son extrémité de réception, stdout et stderr. La délégation d’un fichier de motifs via `-f`, la lecture de plusieurs opérandes et la traversée récursive demandent une définition explicite de leurs droits. Préciser séparément les conditions d’admission en tant que producteur pour `grep … | head`.

La recette proposée couvre deux invités, deux géométries P9 jetables, les statuts 0/1/2, EOF, la sortie immédiate avec `-q`, les écritures bloquées du producteur, la révocation, la collecte des deux enfants et l’absence de droit résiduel. Vérifier le disque et le voisin témoin avant et après les refus. La MOE devra aussi qualifier les moteurs de regex et leur budget mémoire en espace utilisateur. Décision ouverte.
