#!/bin/sh
set -eu

mode="${1:-}"
root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
run() {
  lang="$1"
  shift
  cargo run --quiet --manifest-path "$root/Cargo.toml" -p treetags-codegen -- \
    --grammar "$root/codegen/cpp/shared/grammar.json" \
    --node-types "$root/codegen/cpp/shared/node-types.json" \
    --query "$root/codegen/cpp/$lang/tags.scm" \
    --kinds "$root/codegen/cpp/$lang/kinds.json" \
    --parse "$root/codegen/cpp/shared/parse.json" \
    --output "$root/src/parser/generated/$lang.rs" \
    --module-name "$lang" "$@" $mode
}
run c
# The C++ query is the C-family superset, so emit the shared structural module here.
run cpp --shared-output "$root/src/parser/generated/cfamily.rs"

# Objective-C has its own grammar and parse facts, but emits the same table
# format and runs on the shared C-family island/runtime engines.
cargo run --quiet --manifest-path "$root/Cargo.toml" -p treetags-codegen -- \
  --grammar "$root/codegen/objective_c/grammar.json" \
  --node-types "$root/codegen/objective_c/node-types.json" \
  --query "$root/codegen/objective_c/tags.scm" \
  --kinds "$root/codegen/objective_c/kinds.json" \
  --parse "$root/codegen/objective_c/parse.json" \
  --output "$root/src/parser/generated/objective_c.rs" \
  --module-name objective_c $mode
