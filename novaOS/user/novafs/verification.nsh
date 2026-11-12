#!/bin/nsh
# Vérifier le fichier témoin créé par l’installateur.
if test -f /systeme; then
    echo "Lecture du disque système"
    cat /systeme
else
    return 1
fi
