# Chargé par les scripts de livraison après résolution de here et root.
if [ -z "${NOVAOS_TMPDIR:-}" ]; then
    common=$(git -C "$root" rev-parse --path-format=absolute --git-common-dir)
    NOVAOS_TMPDIR="$(dirname "$common")-worktrees/tmp"
fi
mkdir -p "$NOVAOS_TMPDIR"
NOVAOS_TMPDIR=$(CDPATH='' cd "$NOVAOS_TMPDIR" && pwd -P)
export NOVAOS_TMPDIR TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
