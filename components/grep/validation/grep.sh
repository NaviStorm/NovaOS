#!/bin/zsh -f
# Banc d'exemples pour le grep ARC-027 (binaire Darwin) et GNU grep (Homebrew).
emulate -R zsh
setopt ERR_EXIT PIPE_FAIL
export LC_ALL=C
unset GREP_OPTIONS GREP_COLOR GREP_COLORS POSIXLY_CORRECT

usage() {
    print -r -- "Usage : ${0:t} --novaOS | --macOS"
    print -r -- "  --novaOS   teste le binaire grep livré à côté de ce script, sur le Mac"
    print -r -- "  --macOS    teste /opt/homebrew/bin/ggrep (GNU grep)"
    print -r -- "  Chaque ligne affiche le statut et les arguments du cas testé."
}
if (( $# != 1 )); then usage >&2; exit 2; fi
mode=$1
case $mode in
    --novaOS) exe=${0:A:h}/grep ;;
    --macOS)  exe=/opt/homebrew/bin/ggrep ;;
    --help|-h) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
esac
if [[ ! -x $exe ]]; then print -u2 -r -- "Exécutable introuvable : $exe"; exit 2; fi

export TMPDIR=${NOVAOS_TMPDIR:-/Users/thierry/SynologyDrive/Personnel/StudioProjects/OS-worktrees/tmp}
scratch=$(mktemp -d "$TMPDIR/grep-exemples.XXXXXXXX")
# mktemp réserve ce dossier à cette seule exécution.
trap 'cd / && /bin/rm -R -- "$scratch"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM HUP
cd "$scratch"
mkdir -p arbre/src arbre/cache
printf 'avant\nhit\napres\nvide\nhit fin\n' > journal
printf 'repos\n' > autre
printf 'a.b\naab\naa\nab\n' > expressions
printf 'hit\nHIT\nhitman\n' > mots
printf 'erreur\nhit\n' > motifs
printf 'absent\nhit\0suite\n' > binaire
printf 'un\0hit\0' > nul
printf '%s\n' '-n' > tirets
printf 'TODO\n' > arbre/src/a.rs
printf 'NOTE\n' > arbre/cache/b.txt
printf '*.rs\n' > exclusions

typeset -i ok=0 echecs=0
entree=journal
erreur_attendue=''
# Les sorties attendues sont écrites avec les échappements de printf afin
# de comparer aussi les octets NUL, les tabulations et le dernier saut de ligne.
essai() {
    local attendu_rc=$1 attendu=$2
    shift 2
    local rc=0
    "$exe" "$@" < "$entree" > obtenu 2> diagnostic.txt || rc=$?
    printf 'grep'
    printf ' %q' "$@"
    printf ' < %q  ' "$entree"

    printf '%b' "$erreur_attendue" > attendu.err
    printf '%b' "$attendu" > attendu.out
    # L'aide et la version peuvent changer de texte avec la livraison.
    local conforme=1
    case $1 in
        --help)
            [[ $(<obtenu) == *'--version'* && ! -s diagnostic.txt ]] || conforme=0 ;;
        -V|--version)
            [[ $(<obtenu) == *grep* && ! -s diagnostic.txt ]] || conforme=0 ;;
        -T|--initial-tab)
            [[ $(<obtenu) == *$'\thit' && ! -s diagnostic.txt ]] || conforme=0 ;;
        *)
            cmp -s attendu.out obtenu && cmp -s attendu.err diagnostic.txt || conforme=0 ;;
    esac
    if (( rc == attendu_rc && conforme )); then
        ok+=1
        print -r -- "→ OK ($rc)"
    else
        echecs+=1
        print -r -- "→ ÉCHEC (statut $rc, attendu $attendu_rc)"
        print -r -- "  stdout :"; cat -v obtenu
        print -r -- "  stderr :"; cat -v diagnostic.txt
    fi
}
# Une invocation indépendante pour chaque orthographe de l'option.
formes() {
    local liste=$1 option
    shift
    for option in ${=liste}; do
        essai "$1" "$2" "$option" "${@:3}"
    done
}

print -r -- "Binaire : $exe"
"$exe" --version
entree=expressions formes '-G --basic-regexp' 0 'aab\naa\n' 'a\{2\}'
entree=expressions formes '-E --extended-regexp' 0 'aab\nab\n' '^(aab|ab)$'
entree=expressions formes '-F --fixed-strings' 0 'a.b\n' a.b
entree=expressions formes '-P --perl-regexp' 0 'ab\n' '(?<=^a)b$'
entree=mots formes '-i -y --ignore-case' 0 'hit\nHIT\nhitman\n' HIT
entree=mots formes '--no-ignore-case' 0 'hit\nhitman\n' hit
entree=mots formes '-v --invert-match' 0 'HIT\n' hit
entree=mots formes '-w --word-regexp' 0 'hit\n' hit
formes '-x --line-regexp' 0 'hit\n' hit
formes '-e --regexp' 0 'hit\nhit fin\n' hit -e erreur
formes '-f --file' 0 'hit\nhit fin\n' motifs journal
formes '-n --line-number' 0 '2:hit\n5:hit fin\n' hit
formes '-c --count' 0 '2\n' hit
formes '-l --files-with-matches' 0 'journal\n' hit autre journal
formes '-L --files-without-match' 0 'autre\n' hit autre journal
formes '-o --only-matching' 0 'hit\nhit\n' hit
formes '-q --quiet --silent' 0 '' hit
formes '-s --no-messages' 2 '' hit inexistant
formes '-A --after-context' 0 'hit\napres\n' 1 '^hit$'
formes '-B --before-context' 0 'avant\nhit\n' 1 '^hit$'
formes '-C --context' 0 'avant\nhit\napres\n' 1 '^hit$'
essai 0 'avant\nhit\napres\n' -1 '^hit$'
essai 0 'hit\nSEP\nhit fin\n' --group-separator=SEP -C0 hit
essai 0 'hit\nhit fin\n' --no-group-separator -C0 hit
formes '-r -R --recursive --dereference-recursive' 0 'arbre/src/a.rs:TODO\n' TODO arbre
essai 0 'arbre/src/a.rs:TODO\n' '--include=*.rs' -r . arbre
essai 0 'arbre/cache/b.txt:NOTE\n' '--exclude=*.rs' -r . arbre
essai 0 'arbre/src/a.rs:TODO\n' --exclude-dir=cache -r . arbre
essai 0 'arbre/cache/b.txt:NOTE\n' --exclude-from=exclusions -r . arbre
formes '-d --directories' 1 '' skip TODO arbre
formes '-D --devices' 1 '' skip hit /dev/null
formes '-H --with-filename' 0 'journal:hit\n' '^hit$' journal
formes '-h --no-filename' 0 'hit\n' '^hit$' autre journal
essai 0 'flux:hit\n' --label=flux -H '^hit$'
formes '--color=always --colour=always' 0 '\033[01;31m\033[Khit\033[m\033[K\n' '^hit$'
entree=binaire formes '-a --text --binary-files=text' 0 'hit\000suite\n' hit
entree=binaire formes '-I --binary-files=without-match' 1 '' hit
entree=binaire erreur_attendue="${exe:t}: (standard input): binary file matches\\n" essai 0 '' --binary-files=binary hit
entree=nul formes '-z --null-data' 0 'hit\000' hit
essai 0 'journal\000' --null -l hit journal
formes '-b --byte-offset' 0 '6:hit\n' -o '^hit$'
formes '-T --initial-tab' 0 '' -Hn '^hit$'
formes '-m --max-count' 0 'hit\n' 1 hit
essai 0 'hit\nhit fin\n' --line-buffered hit
formes '-U --binary' 0 'hit\n' '^hit$'
entree=tirets essai 0 '-n\n' -- -n
essai 1 '' introuvable journal
formes '-V --version' 0 ''

essai 0 'journal\000' -Z -l hit journal
essai 0 '' --help
print -r -- "Bilan : $ok réussis, $echecs échecs."
(( echecs == 0 ))
