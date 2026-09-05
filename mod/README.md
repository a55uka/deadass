# deadass mod

Panorama client addon bridging Deadlock match events to the companion.

The stock `citadel_hud_top_bar` layout (loaded every match) pulls in
`deadass.js`, which scrapes HUD panels — the local player's `Dead` class,
the kill-streak popup counter, the damage-impact assist flags, and the
`hud_signature` ability containers — and writes `[DEADASS]{...}` JSON
records to the game console via `$.Msg`. The companion tails
`game/citadel/console.log` (requires `-condebug` in Deadlock's Steam
launch options) and converts records into haptic triggers.

Build with `just build` (needs the Reduced CSDK), install the resulting
`dist/deadass.vpk` via the Deadlock Mod Manager or
`Deadlock/game/addons`.
