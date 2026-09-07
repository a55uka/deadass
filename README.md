# deadass

Deadlock buttplug.io bridge that drives your touys,.,

```
mod/        Deadlock Panorama addon (HUD scrapers -> [DEADASS] JSON on console.log)
shared/     deadass-shared: GameEvent, TriggerKind, AppConfig
companion/  deadass-companion: log tail + HTTP ingress -> dedup -> haptic gate -> ToyHub
desktop/    deadass-desktop: Tauri shell over the companion pipeline + vanilla frontend
scripts/    CSDK VPK builders invoked by the justfile
```

Requires `-condebug` in Deadlock's launch options so `console.log` exists.

```sh
just build           # compile Panorama mod sources and pack dist/deadass.vpk
just pack            # pack without the CSDK (zip fallback) (this wont work for the game)
just companion-run   # debug build + launch the headless companion
just desktop-run     # launch the Tauri desktop UI (and the companion)
just rust-test       # cargo test --workspace
```

Toy backends: embedded in-process server, or external Intiface Central
over `buttplug_ws_url`. Set `DEADASS_TOYS=embedded|central` to autoconnect.
