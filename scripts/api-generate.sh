#!/usr/bin/env bash
# Generates the cloud API clients from the API contract:
#   src/cloud/schema.d.ts           TypeScript types for openapi-fetch / React Query
#   src-tauri/src/cloud/generated.rs Rust client (progenitor, reqwest)
#
# The contract is libs/api-contract/openapi.yaml, kept in sync by the
# "Sync API contract" pull requests. To generate from another copy, for
# example an unreleased contract while developing the API, set AUGRY_OPENAPI:
#   AUGRY_OPENAPI=../Terminal-Grid-Cloud/libs/api-spec/openapi.yaml npm run api:generate
set -euo pipefail
cd "$(dirname "$0")/.."

spec="${AUGRY_OPENAPI:-libs/api-contract/openapi.yaml}"
if [ ! -f "$spec" ]; then
  echo "api:generate: no contract at $spec. Merge the \"Sync API contract\" pull request, or set AUGRY_OPENAPI." >&2
  exit 1
fi

npx openapi-typescript "$spec" -o src/cloud/schema.d.ts
mkdir -p src-tauri/src/cloud
cargo run --quiet --manifest-path tools/api-codegen/Cargo.toml -- "$spec" src-tauri/src/cloud/generated.rs
# Format like `cargo fmt` so formatting checks pass on the generated file.
rustfmt --edition 2021 src-tauri/src/cloud/generated.rs
