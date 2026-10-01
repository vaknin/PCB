#!/usr/bin/env bash
# Checks that the pcb-pipeline skill (.claude/skills/pcb-pipeline/*.md) names only things that
# exist, so the skill can't drift from the code without a red line here.
#
#   scripts/skill-check.sh            check the skill; exit 1 and list what is missing
#
# From every `backtick span` and every line of a ``` code block it takes:
#   - paths under scripts/, docs/, templates/, crates/, research/, enclosure/, lib/,
#     firmware/components/, firmware/test/, boards/starter/, boards/capture-clip/ (no <placeholders>)
#     -> must exist on disk
#   - `cargo run ... -- <words>`: each lower-case word is a stage or command of the target
#     (pcbgen board: STAGES in crates/pcbgen/src/cli.rs; -p devctl: crates/devctl/src/main.rs;
#     --bin pcb: crates/pcbgen/src/bin/pcb*), each --flag after `--` a flag of that target
#   - any other --flag -> must appear in pcbgen's cli.rs, devctl, the pcb binary, scripts/ or a
#     board's firmware/sim/run.py (or be cargo's own: --release, --bin, --ignored)
#   - board.toml tables `[x]` / `[[x.y]]`, `[x] key` and `key = ...` -> in templates/board.toml
#     (commented examples count) or as a field in crates/pcbgen/src/boardfile.rs
# Lines between `<!-- pending ... -->` (at the start of a line) and `<!-- /pending -->`, or a
# line carrying an inline `<!-- pending ... -->`, describe tools not merged yet: their misses
# are listed as pending and do not fail.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
skill="$root/.claude/skills/pcb-pipeline"
cli="$root/crates/pcbgen/src/cli.rs"
devctl="$root/crates/devctl/src/main.rs"
boardfile="$root/crates/pcbgen/src/boardfile.rs"
template="$root/templates/board.toml"
shopt -s nullglob
pcbbin=("$root"/crates/pcbgen/src/bin/pcb*)

[[ -d "$skill" ]] || { echo "== SKILL-CHECK: FAIL (no $skill)"; exit 1; }

missing=()
pending=()
checked=0

# mode, where, what: records one miss
miss() {
    if [[ $1 == pending ]]; then pending+=("$2: $3"); else missing+=("$2: $3"); fi
}

# the pcb binary: src/bin/pcb.rs and its command modules in src/bin/pcb/
if ((${#pcbbin[@]})); then
    mapfile -t pcbfiles < <(find "${pcbbin[@]}" -name '*.rs')
else
    pcbfiles=()
fi
# Rust and script sources a prose --flag may come from
flag_sources=("$cli" "$devctl" "${pcbfiles[@]}" "$root"/scripts/*.sh "$root"/scripts/*/*.sh
              "$root"/boards/*/firmware/sim/run.py)

has_flag() { # target, flag
    case $1 in
        pcbgen) grep -qF "\"$2\"" "$cli" ;;
        devctl) grep -qF "\"$2\"" "$devctl" ;;
        pcb) ((${#pcbfiles[@]})) && grep -qF "\"$2\"" "${pcbfiles[@]}" ;;
        any) [[ $2 =~ ^--(release|bin|ignored)$ ]] || grep -qwF -- "$2" "${flag_sources[@]}" ;;
    esac
}

has_word() { # target, word
    case $1 in
        pcbgen) grep -E '^pub const STAGES' "$cli" | grep -qF "\"$2\"" ;;
        devctl) grep -qF "\"$2\"" "$devctl" ;;
        pcb) ((${#pcbfiles[@]})) && grep -qF "\"$2\"" "${pcbfiles[@]}" ;;
    esac
}

has_table() { # dotted name
    local t=$1 last=${1##*.}
    grep -qE "^#? *\[\[?${t//./\\.}\]\]?" "$template" || grep -qE "pub ${last}: " "$boardfile"
}

has_key() {
    grep -qE "^#? *$1 *=" "$template" || grep -qE "pub $1: " "$boardfile"
}

check_span() { # mode, where, span
    local mode=$1 where=$2 s=$3 p w part target rest t k words parts
    # paths
    while read -r p; do
        [[ -z "$p" ]] && continue
        p=${p%.}
        checked=$((checked + 1))
        [[ -e "$root/$p" ]] || miss "$mode" "$where" "path $p"
    done < <(grep -oP '(?<![A-Za-z0-9_./<>*-])(scripts|docs|templates|crates|research|enclosure|lib|firmware/components|firmware/test|boards/starter|boards/capture-clip)/[^\s`"'"'"'),;:|]*' <<<"$s" \
             | grep -vE '[<>*{$]|/\.venv' || true)

    if [[ $s == *"cargo run"*" -- "* ]]; then
        target=pcbgen
        [[ $s =~ -p\ devctl ]] && target=devctl
        [[ $s =~ --bin\ pcb ]] && target=pcb
        rest=${s#* -- }
        read -ra words <<<"$rest"
        for w in "${words[@]}"; do
            w=${w//[\[\]]/}
            IFS='|' read -ra parts <<<"$w"
            for part in "${parts[@]}"; do
                if [[ $part =~ ^--[a-z][a-z0-9-]*$ ]]; then
                    checked=$((checked + 1))
                    has_flag "$target" "$part" || miss "$mode" "$where" "$target flag $part"
                elif [[ $part =~ ^[a-z][a-z_]*$ ]]; then
                    checked=$((checked + 1))
                    has_word "$target" "$part" || miss "$mode" "$where" "$target stage/command $part"
                fi
            done
        done
        return
    fi

    mapfile -t words < <(grep -oE '(^|[^A-Za-z0-9-])--[a-z][a-z0-9-]*' <<<"$s" | grep -oE -- '--[a-z][a-z0-9-]*' || true)
    for w in "${words[@]}"; do
        checked=$((checked + 1))
        has_flag any "$w" || miss "$mode" "$where" "flag $w"
    done

    # board.toml tables, optionally followed by a key: [case] material, [[pin]] sim
    while read -r t; do
        [[ -z "$t" ]] && continue
        k=""
        [[ $t == *" "* ]] && k=${t##* }
        t=${t%% *}
        t=${t//[\[\]]/}
        checked=$((checked + 1))
        has_table "$t" || miss "$mode" "$where" "board.toml table [$t]"
        if [[ -n "$k" ]]; then
            checked=$((checked + 1))
            has_key "$k" || miss "$mode" "$where" "board.toml key $k in [$t]"
        fi
    done < <(grep -oE '\[\[?[a-z][a-z_.]*\]\]?( [a-z][a-z_]*)?' <<<"$s" || true)

    # key = value (a board.toml line); Cargo.toml keys are not board.toml's
    if [[ $s =~ ^([a-z][a-z_]*)\ = ]]; then
        k=${BASH_REMATCH[1]}
        if [[ ! $k =~ ^(edition|pcbgen|version)$ ]]; then
            checked=$((checked + 1))
            has_key "$k" || miss "$mode" "$where" "board.toml key $k"
        fi
    fi
}

# One record per span: mode<TAB>file:line<TAB>span
spans=$(awk '
    FNR == 1 { if (inpend) { print "ERROR\t" prev ":end\tunclosed <!-- pending --> block"; } inpend = 0; fence = 0 }
    { prev = FILENAME; sub(/.*\//, "", prev); where = prev ":" FNR }
    /^<!-- *\/pending *-->/ { inpend = 0; next }
    /^<!-- *pending/ { inpend = 1; next }
    {
        mode = (inpend || /<!-- *pending/) ? "pending" : "active"
        if ($0 ~ /^```/) { fence = !fence; next }
        if (fence) { print mode "\t" where "\t" $0; next }
        line = $0
        while (match(line, /`[^`]+`/)) {
            print mode "\t" where "\t" substr(line, RSTART + 1, RLENGTH - 2)
            line = substr(line, RSTART + RLENGTH)
        }
    }
    END { if (inpend) print "ERROR\t" prev ":end\tunclosed <!-- pending --> block" }
' "$skill"/*.md)

while IFS=$'\t' read -r mode where span; do
    [[ -z "$mode" ]] && continue
    if [[ $mode == ERROR ]]; then
        missing+=("$where: $span")
        continue
    fi
    check_span "$mode" "$where" "$span"
done <<<"$spans"

echo "== SKILL-CHECK ($(ls "$skill"/*.md | wc -l) files, $checked items)"
for p in "${pending[@]}"; do echo "   pending  $p"; done
for m in "${missing[@]}"; do echo "   MISSING  $m"; done
if ((${#missing[@]})); then
    echo "== SKILL-CHECK: FAIL (${#missing[@]} missing; fix the skill or the code)"
    exit 1
fi
echo "== SKILL-CHECK: PASS (${#pending[@]} pending)"
