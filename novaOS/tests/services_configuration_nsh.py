"""Contrat de la commande service dans une session nsh P9 ordinaire."""


def verifie_console_ordinaire(vm, commande):
    def exige(ligne, *marques):
        texte = commande(vm, ligne)
        for marque in marques:
            assert marque in texte, (ligne, marque, texte)

    exige("help", "rmdir service sha256sum tail")
    exige("service --help", "service list", "enable|disable")
    exige("service list", "service=storage-info id=9", "service=cache id=10")
    exige("service policy storage-info", "restart=failure", "retries=2")
    exige("service dependencies cache", "dépendance=storage-info id=9 obligatoire")
    exige("nsh -c 'service status storage-info; echo statut=$?'", "Current state: RUNNING", "statut=0")
    exige("nsh -c 'service disable 9; echo refus=$?'", "accès refusé", "refus=5")
    exige("nsh -c 'service --invalide; echo syntaxe=$?'", "argument invalide", "syntaxe=3")
