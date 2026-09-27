#!/usr/bin/env bash
# Reconstruit un pilote du dépôt avec le nécessaire (sdk/), dans un dossier
# à part, et donne l'empreinte SHA-256 de l'ELF. Même Rust (1.98.1), même
# source : même empreinte, octet pour octet, sur toute machine.
# Usage : outils/construire.sh <nom>
set -euo pipefail
cd "$(dirname "$0")/.."
nom=${1:?nom du pilote}
[[ $nom =~ ^[a-z][a-z0-9-]{0,23}$ ]] || { echo "nom invalide" >&2; exit 1; }
[ -f "pilotes/$nom/Cargo.toml" ] || { echo "pilotes/$nom absent" >&2; exit 1; }
[ ! -e "pilotes/$nom/build.rs" ] || { echo "build.rs refusé" >&2; exit 1; }
t=target/construction/$nom
rm -rf "$t"; mkdir -p "$t"
cp -r sdk/. "$t/"
mkdir -p "$t/programs/$nom"
cp -r "pilotes/$nom/Cargo.toml" "pilotes/$nom/src" "$t/programs/$nom/"
sed -i "s#\"programs/modele\"\]#\"programs/modele\", \"programs/$nom\"]#" "$t/Cargo.toml"
(cd "$t" && bash construire.sh >/dev/null)
elf="$t/target/programs/x86_64-unknown-none/release/$nom"
echo "$(sha256sum "$elf" | cut -d' ' -f1)  $nom"
