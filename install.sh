#!/usr/bin/env bash
set -euo pipefail

repo_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
cargo_home=${CARGO_HOME:-"$HOME/.cargo"}

cargo install --path "$repo_dir" --root "$cargo_home" --force
printf 'installed cxq to %s/bin/cxq\n' "$cargo_home"

