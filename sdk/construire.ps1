# Compile les programmes du nécessaire (modele, et les vôtres) pour Aiwos,
# sous Windows : la même chose que construire.sh.
#
# Usage : pwsh construire.ps1 [options de cargo]
#   (ou : powershell -ExecutionPolicy Bypass -File construire.ps1)
# L'ELF : target\programs\x86_64-unknown-none\release\<nom>
$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot -replace '\\', '/'
# Options séparées par 0x1f : un chemin peut contenir des espaces.
$flags = @(
    '-Crelocation-model=static',
    "-Clink-arg=-T$root/programs/user.ld",
    "--remap-path-prefix=$root=/aiwos"
) -join [char]0x1f
$old = $env:CARGO_ENCODED_RUSTFLAGS
Push-Location $PSScriptRoot
try {
    $env:CARGO_ENCODED_RUSTFLAGS = $flags
    cargo build --release --target x86_64-unknown-none --target-dir target/programs @args
    $code = $LASTEXITCODE
} finally {
    $env:CARGO_ENCODED_RUSTFLAGS = $old
    Pop-Location
}
exit $code
