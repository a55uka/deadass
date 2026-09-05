# Build the deadass Deadlock addon and Rust workspace.
#
# Windows: runs scripts/build.ps1 with PowerShell (native CSDK tools).
# Linux:   runs scripts/build.sh, which runs the CSDK tools under Proton
#          (via protontricks-launch; plain Wine can't init D3D11).
#
# Usage:
#   just build               # build dist/deadass.vpk
#   just build out.vpk       # build to a different path
#   just pack                # pack dist/deadass.vpk without the CSDK (zip fallback)
#   just clean               # remove dist/ and CSDK build dirs
#   just companion-run       # debug build + launch the companion
#   just dll-build           # build the Windows fidelity DLL (see recipe)
#
# Override the CSDK location with `csdk=...` or $DEADLOCK_CSDK.
# On Windows an empty value lets scripts/build.ps1 auto-detect the install.

csdk := if os_family() == "windows" { env_var_or_default('DEADLOCK_CSDK', '') } else { env_var_or_default('DEADLOCK_CSDK', '/home/cat/Documents/Reduced_CSDK_12') }
output := 'dist/deadass.vpk'

# compile Panorama sources and pack the VPK
build out=output:
    @just _build-{{os_family()}} "{{out}}"

# Windows path: native tools via scripts/build.ps1 (powershell ships with Windows;
# swap to pwsh if you prefer PowerShell 7 — the script is compatible).
_build-windows out:
    powershell -NoProfile -ExecutionPolicy Bypass -File "{{justfile_directory()}}/scripts/build.ps1" -CsdkRoot "{{csdk}}" -OutputPath "{{out}}"

# Linux path: CSDK tools under Proton.
_build-unix out:
    "{{justfile_directory()}}/scripts/build.sh" "{{csdk}}" "{{justfile_directory()}}" "{{out}}"

# pack without the CSDK (plain zip, no Panorama compile step)
pack out=output:
    "{{justfile_directory()}}/mod/tools/build_vpk.sh" "{{justfile_directory()}}/{{out}}"

# remove build artifacts
clean:
    @just _clean-{{os_family()}}

_clean-windows:
    powershell -NoProfile -Command "$csdk='{{csdk}}'; Remove-Item '{{justfile_directory()}}/dist' -Recurse -Force -ErrorAction SilentlyContinue; if ($csdk) { Remove-Item (Join-Path $csdk 'content/citadel_addons/deadass'), (Join-Path $csdk 'game/citadel_addons/deadass') -Recurse -Force -ErrorAction SilentlyContinue }"

_clean-unix:
    #!/usr/bin/env bash
    set -euo pipefail
    rm -rf "{{justfile_directory()}}/dist"
    if [ -n "{{csdk}}" ]; then
        rm -rf "{{csdk}}/content/citadel_addons/deadass" "{{csdk}}/game/citadel_addons/deadass"
    fi

# ---- Rust workspace (native cargo, same on Windows and Linux) ----

# debug build of the whole workspace (pass extra cargo args, e.g. `just rust-build --release`)
rust-build *args:
    cargo build {{args}}

# debug build + launch the companion
companion-run *args:
    cargo run -p deadasss-companion {{args}}

# release build of the companion
companion-release *args:
    cargo build --locked --release -p deadasss-companion {{args}}

# build the fidelity DLL (native target; cross-compile to Windows with cargo-xwin)
dll-build *args:
    cargo build -p deadass-dll {{args}}

# Rust test suite (whole workspace)
rust-test *args:
    cargo test --workspace {{args}}

# fast typecheck without producing binaries
rust-check *args:
    cargo check --workspace {{args}}

default:
    @just --list
