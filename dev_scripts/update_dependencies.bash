#!/usr/bin/env bash
# Updates Cargo dependencies and generates a commit if the operation is successful and tests pass.
# This requires the user to have the following binaries available in their PATH:
#
# - cargo
# - cargo-upgrade
# - cargo-update
# - cargo-nextest
#
# If these can't be found the script will exit with an error.

required_binaries=("cargo" "cargo-upgrade" "cargo-nextest")

for cmd in "${required_binaries[@]}"
do
  if ! command -v "$cmd"
  then
    echo "$cmd was not found"
    exit 1
  fi
done

# Checks if the current branch is clean, we only want this script to run in a clean context so
# we don't accidentally commit other changes.
has_changes=$(git diff-files --quiet)

if [ "$has_changes" -ne "0" ]; then
  echo "ERROR: detected local changes. You must run this script with a clean git context."
  exit 1
fi

set -ex

cargo upgrade --incompatible
cargo update
cargo test --doc
cargo nextest run
git add Cargo.lock Cargo.toml
git commit -m "chore(deps): Update cargo dependencies" \
  -m "Done with \`dev_scripts/update_dependencies.bash\`"
