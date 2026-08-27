#!/usr/bin/env bash
# Byte-identity gate for the water-distribution session (docs/plans/
# wds-interleaved-quality.md). Runs each model and records the md5 of its
# .out. Interleaving quality with hydraulics reorders when quality is
# computed; it must not change what is computed, and a diff of these sums is
# the only proof of that.
#
#   scripts/wds_byte_gate.sh record  > baseline.txt   # before the change
#   scripts/wds_byte_gate.sh check   baseline.txt     # after
set -euo pipefail
HYDRA=${HYDRA:-target/release/hydra}
# A FIXED work directory, not mktemp: the .out prolog stores the report
# filename in a 260-byte field, so a path that varies per run varies the
# sums and the gate compares nothing.
WORK=${WDS_GATE_WORK:-${TMPDIR:-/tmp}/hydra-wds-gate}
rm -rf "$WORK"; mkdir -p "$WORK"
MODELS=(dtown nytunnels ky8 ky9 ky10 ltown micropolis richmond bwsn2 exnet balerma)

sums() {
  for m in "${MODELS[@]}"; do
    f="tests/benchmarks/wds/$m.inp"
    [ -f "$f" ] || continue
    "$HYDRA" run "$f" -q --results "$WORK/$m.out" --summary "$WORK/$m.rpt" >/dev/null 2>&1 || {
      echo "$m ERROR"; continue; }
    echo "$m $(md5 -q "$WORK/$m.out")"
  done
}

case "${1:-}" in
  record) sums ;;
  check)
    [ -f "${2:-}" ] || { echo "usage: $0 check <baseline>" >&2; exit 2; }
    now=$(sums); bad=0
    while read -r m want; do
      got=$(echo "$now" | awk -v m="$m" '$1==m{print $2}')
      if [ "$got" = "$want" ]; then printf "  ok    %-12s %s\n" "$m" "$want"
      else printf "  MOVED %-12s %s -> %s\n" "$m" "$want" "${got:-<missing>}"; bad=1; fi
    done < "$2"
    [ "$bad" = 0 ] && echo "every model byte-identical" || { echo "RESULTS MOVED"; exit 1; }
    ;;
  *) echo "usage: $0 record|check <baseline>" >&2; exit 2 ;;
esac
