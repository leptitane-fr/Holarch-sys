#!/usr/bin/env bash
# Compile les programmes du nécessaire (modele, et les vôtres) pour Holarch :
# Rust 1.98.1 (rust-toolchain.toml), cible x86_64-unknown-none, adresses fixes (user.ld). Le
# chemin de ce dossier devient /aiwos dans l'ELF : deux dossiers
# différents donnent le même programme (avec le même Rust).
#
# Usage : bash construire.sh [options de cargo]
# L'ELF : target/programs/x86_64-unknown-none/release/<nom>
set -euo pipefail
cd "$(dirname "$0")"
# Sous Git Bash (Windows), $PWD vaut /c/…, que cargo ne sait pas lire.
root=$(pwd -W 2>/dev/null || pwd)
# Options séparées par 0x1f : un chemin peut contenir des espaces.
CARGO_ENCODED_RUSTFLAGS=$(printf '%s\037%s\037%s' \
    "-Crelocation-model=static" \
    "-Clink-arg=-T$root/programs/user.ld" \
    "--remap-path-prefix=$root=/aiwos")
export CARGO_ENCODED_RUSTFLAGS
cargo build --release --target x86_64-unknown-none --target-dir target/programs "$@"
