# deadass

Deadlock buttplug.io bridge that drives your touys,.,

```
mod/        Deadlock Panorama addon (HUD scrapers -> [DEADASS] JSON on console.log)
shared/     deadass-shared: GameEvent, TriggerKind, AppConfig, DataSource
companion/  deadass-companion: log tail + HTTP ingress + DLL TCP ingress -> dedup -> haptic gate -> ToyHub
dll/        deadass-dll: read-only in-process reader injected into deadlock.exe
desktop/    deadass-desktop: Tauri shell over the companion pipeline + vanilla frontend
scripts/    CSDK VPK builders invoked by the justfile
```

## Game data sources

There are two ways for this mod to collect data:

- **dll** (default) — the companion injects `deadass_dll.dll` into the running game
  (LoadLibraryW remote thread, ~2 s supervisor loop) and the DLL reads the
  local player pawn and ability state directly from client.dll memory and
  streams newline-delimited JSON to the companion over `127.0.0.1`
  (port `dll_event_port`, default 24680).
- **mod** — the panorama addon scrapes the HUD and prints
  `[DEADASS]` JSON into Deadlock's console; the companion tails
  `game/citadel/console.log`. Needs `-condebug` in Deadlock's launch options.
  Detects kill/death/assist/respawn/ability events. (honestly dont use this, its just overall worse than the dll collector)

DLL memory offsets come from [dezlock-dump](../dezlock-dump).

```sh
just build           # compile Panorama mod sources and pack dist/deadass.vpk
just pack            # pack without the CSDK (zip fallback) (this wont work for the game)
just dll-build       # build target/<profile>/deadass_dll.dll
just companion-run   # debug build + launch the headless companion
just desktop-run     # launch the Tauri desktop UI (and the companion)
just rust-test       # cargo test --workspace
```
