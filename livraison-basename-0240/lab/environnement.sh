# Charger depuis un script qui a calculé root et here.
export NOVAOS_TMPDIR="${BASENAME_TMPDIR:-$(dirname "$root")/tmp/basename-arc027}"
mkdir -p "$NOVAOS_TMPDIR"
export TMPDIR="$NOVAOS_TMPDIR/" PYTHONDONTWRITEBYTECODE=1
export RUSTC="$(rustup which --toolchain stable rustc)"
