#!/bin/sh
set -eu
here=$(CDPATH= cd "$(dirname "$0")" && pwd)
case "$("$here/bin/date" --help)" in *"Afficher ou régler"*) lang=fr;; *) lang=en;; esac
exec man -M "$here/man/$lang" 1 date
