# deadass mod

Panorama client addon bridging Deadlock match events to the Linux companion.

Layout mounts `deadass.js`, which diffs `Game.GetLocalPlayerInfo()` plus ability
cooldown state and `POST`s `GameEvent` JSON to `http://127.0.0.1:24681/event`.

Build with `tools/build_vpk.sh`, install the resulting `deadass.vpk` via the
Deadlock Mod Manager or `Deadlock/game/addons`.
