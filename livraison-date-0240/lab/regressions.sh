#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."
export NOVAOS_TMPDIR="${NOVAOS_TMPDIR:-$(cd .. && pwd)/tmp}" TMPDIR="${NOVAOS_TMPDIR:-$(cd .. && pwd)/tmp}/"
out=${1:-"$NOVAOS_TMPDIR/date-arc027/regressions"}
mkdir -p "$out"
out=$(cd "$out" && pwd)
if test "${DATE_REG_LOCKED:-0}" = 0; then
    exec python3 -B -c 'import fcntl,os,subprocess,sys
with open(os.environ["NOVAOS_TMPDIR"]+"/arc027-mesures.lock","a") as lock:
    fcntl.flock(lock,fcntl.LOCK_EX)
    sys.exit(subprocess.call(["sh",*sys.argv[1:]],env=dict(os.environ,DATE_REG_LOCKED="1")))' "livraison-date-0240/lab/regressions.sh" "$out"
fi
: > "$out/statuts.txt"
rc=0
for arch in aarch64 x86_64; do
    for suite in test elf-test lifecycle-test ipc-races block-test cli-test cli-p9-test nsh-test nsh-scripts-test nsh-terminal-test vi-test; do
        code=0
        make -C novaOS ARCH="$arch" "$suite" > "$out/$arch-$suite.log" 2>&1 || code=$?
        echo "$arch $suite $code" >> "$out/statuts.txt"
        test "$code" = 0 || rc=1
    done
done
exit "$rc"
