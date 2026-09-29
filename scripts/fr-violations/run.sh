#!/usr/bin/env bash
# List the clearance violations Freerouting sees in a DSN, with the two items and where
# they are (Freerouting's own reader; see DECISIONS.md D-018).
# usage: scripts/fr-violations/run.sh <board.dsn> [REF,REF,...]   (refs: also list their pins)
# Needs a javac >= 21 on PATH (only to compile; the probe runs on Freerouting's own Java 25
# runtime through a copy of its launcher, since javac 21 can't read Java 25 class files,
# hence the reflection).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
fr=$(cd "$here/../.." && pwd)/tools/freerouting-2.4.1-linux-x64
[ -d "$fr" ] || { echo "Freerouting not found at $fr (scripts/fetch-tools.sh)" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/bin" "$tmp/lib/app/cls"
cp "$fr/bin/freerouting" "$tmp/bin/probe"
cp "$fr/lib/libapplauncher.so" "$tmp/lib/"
ln -s "$fr/lib/runtime" "$tmp/lib/runtime"
ln -s "$fr/lib/app/freerouting-executable.jar" "$tmp/lib/app/freerouting-executable.jar"
printf '[Application]\napp.classpath=$APPDIR/freerouting-executable.jar\napp.classpath=$APPDIR/cls\napp.mainclass=Probe\n\n[JavaOptions]\njava-options=-Dlog4j2.disableJndi=true\n' > "$tmp/lib/app/probe.cfg"
javac --release 21 -d "$tmp/lib/app/cls" "$here/Probe.java"
"$tmp/bin/probe" "$(realpath "$1")" ${2:-} 2>&1 | grep -v -e '^\s*at ' -e ' INFO ' -e ' DEBUG '
