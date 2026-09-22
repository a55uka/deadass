(() => {
  if (window.__TAURI__) return;

  const trigger = (kind, strength, duration_ms, enabled = true) => ({
    kind, enabled, strength, duration_ms, retrigger_cooldown_ms: 500, pattern: "vibrate",
    vibrate_devices: null,
    shock_enabled: false, shock_intensity: 20, shock_duration_ms: 300,
    shock_kind: "shock", shock_shocker_ids: null,
  });

  const triggers = [
    trigger("kill", 0.7, 900),
    trigger("death", 0.4, 1500),
    trigger("assist", 0.5, 700),
    trigger("respawn", 0.3, 500),
    trigger("parry", 0.7, 250),
    trigger("parried", 0.5, 400),
    trigger("punch_landed", 0.45, 200),
    trigger("punch_taken", 0.4, 200),
  ];
  Object.assign(triggers[0], {
    shock_enabled: true, shock_intensity: 25, shock_duration_ms: 300,
    shock_shocker_ids: ["abc123"],
  });
  Object.assign(triggers[4], {
    shock_enabled: true, shock_intensity: 60, shock_duration_ms: 300,
    shock_kind: "vibrate", shock_shocker_ids: ["abc123", "def456"],
  });
  for (let slot = 0; slot < 4; slot += 1) {
    triggers.push(trigger(`ability_used:${slot}`, 0.55, 400));
    triggers.push(trigger(`ability_ready:${slot}`, 0.35, 300));
  }

  const state = {
    data_source: "dll",
    mod_active: true,
    dll_active: true,
    inject_phase: "injected",
    inject_detail: "deadass_dll.dll loaded",
    log_phase: "tailing",
    log_path: "C:/Program Files (x86)/Steam/userdata/logs/console.log",
    toy_mode: "Embedded",
    devices: ["Loonie (ex. Lovense)", "Edge"],
    toy_error: null,
    config_rev: 1,
    config_path: "C:/Users/you/AppData/Roaming/deadass/config.toml",
    log_lines: [
      "tailing C:/Program Files (x86)/Steam/userdata/logs/console.log",
      "embedded toy engine ready devices=1",
      "vibrate kill strength=0.70 duration_ms=900 pattern=Vibrate",
      "suppress death (retrigger cooldown)",
      "config reloaded from C:/Users/you/AppData/Roaming/deadass/config.toml",
      "master_gain 1.00 -> 0.85",
    ],
    config: {
      master_gain: 1,
      max_strength_cap: 1,
      mute_while_dead: false,
      debug_logging: false,
      mod_http_port: 24681,
      dll_event_port: 24680,
      buttplug_ws_url: "ws://127.0.0.1:12345",
      dll_path: null,
      openshock_enabled: true,
      openshock_api_token: "",
      openshock_base_url: "https://api.openshock.app",
      openshock_ready: false,
      openshock_shockers: [
        { id: "abc123", name: "Left hand" },
        { id: "def456", name: "Collar" },
      ],
    },
  };

  function push(line) {
    state.log_lines.push(line);
    if (state.log_lines.length > 200) state.log_lines.shift();
  }

  function snapshot() {
    return {
      ...state,
      triggers: triggers.map((t) => ({
        ...t,
        shock_shocker_ids: t.shock_shocker_ids == null ? null : [...t.shock_shocker_ids],
      })),
      log_lines: [...state.log_lines],
      log: state.log_lines.at(-1) ?? "",
      config: {
        ...state.config,
        openshock_shockers: state.config.openshock_shockers.map((s) => ({ ...s })),
      },
      devices: [...state.devices],
    };
  }

  function findTrigger(kind) {
    return triggers.find((t) => t.kind === kind);
  }

  window.__demoBridge = async (command, args = {}) => {
    await new Promise((resolve) => setTimeout(resolve, 120));
    switch (command) {
      case "get_status":
        return snapshot();
      case "set_data_source":
        state.data_source = args.source;
        if (args.source === "dll") {
          state.inject_phase = "injected";
          state.inject_detail = "deadass_dll.dll loaded";
          state.dll_active = true;
        } else {
          state.inject_phase = "idle";
          state.inject_detail = null;
          state.dll_active = false;
        }
        push(`game data source set to ${args.source}`);
        return snapshot();
      case "set_trigger": {
        const rule = findTrigger(args.kind);
        if (rule) {
          const rosterIds = state.config.openshock_shockers.map((shocker) => shocker.id);
          Object.assign(rule, {
            enabled: args.enabled,
            strength: args.strength,
            duration_ms: args.duration_ms,
            retrigger_cooldown_ms: args.retrigger_cooldown_ms,
            pattern: args.pattern,
            vibrate_devices: Array.isArray(args.vibrate_devices)
              ? args.vibrate_devices
              : rule.vibrate_devices,
            shock_enabled: args.shock_enabled ?? rule.shock_enabled,
            shock_intensity: args.shock_intensity ?? rule.shock_intensity,
            shock_duration_ms: args.shock_duration_ms ?? rule.shock_duration_ms,
            shock_kind: args.shock_kind ?? rule.shock_kind,
            shock_shocker_ids: Array.isArray(args.shock_shocker_ids)
              ? args.shock_shocker_ids.filter((id) => rosterIds.includes(id))
              : rule.shock_shocker_ids,
          });
        }
        state.config_rev += 1;
        push(`trigger ${args.kind} updated`);
        return snapshot();
      }
      case "set_config": {
        for (const [key, value] of Object.entries(args)) {
          if (key in state.config) state.config[key] = value;
        }
        if (args.dll_path === "") state.config.dll_path = null;
        if (Array.isArray(args.openshock_shockers)) {
          state.config.openshock_shockers = args.openshock_shockers
            .map((shocker) => ({ id: shocker.id.trim(), name: shocker.name.trim() }))
            .filter((shocker) => shocker.id);
          const ids = state.config.openshock_shockers.map((shocker) => shocker.id);
          for (const rule of triggers) {
            if (Array.isArray(rule.shock_shocker_ids)) {
              rule.shock_shocker_ids = rule.shock_shocker_ids.filter((id) => ids.includes(id));
            }
          }
        }
        state.config.openshock_ready = Boolean(
          state.config.openshock_enabled &&
          state.config.openshock_api_token &&
          state.config.openshock_shockers.length,
        );
        state.config_rev += 1;
        push("config updated");
        return snapshot();
      }
      case "test_toys": {
        if (state.toy_mode === "Disconnected" || !state.devices.length) {
          push("toys test: no toys connected");
          return snapshot();
        }
        push(`toys test: vibrate 50% 500ms on ${state.devices.length} toy${state.devices.length === 1 ? "" : "s"}`);
        return snapshot();
      }
      case "test_openshock": {
        const ids = state.config.openshock_shockers.map((shocker) => shocker.id);
        if (!state.config.openshock_enabled || !state.config.openshock_api_token || !ids.length) {
          push("openshock test: not configured (enable it, set a token, add a shocker)");
          return snapshot();
        }
        push(`openshock test fired on ${ids.length} shocker(s): intensity=20 duration=300ms`);
        return snapshot();
      }
      case "test_fire": {
        const rule = findTrigger(args.kind);
        if (!rule || !rule.enabled) {
          push(`test ${args.kind}: disabled in config`);
          return snapshot();
        }
        const targets = rule.vibrate_devices == null
          ? state.devices
          : state.devices.filter((name) => rule.vibrate_devices.includes(name));
        if (targets.length) {
          push(`test ${args.kind}: vibrate strength=${Number(rule.strength).toFixed(2)} duration_ms=${rule.duration_ms} pattern=${rule.pattern} toys=${targets.length} [${targets.join(", ")}]`);
        } else {
          push(`test ${args.kind}: vibration skipped, no target toys selected or connected`);
          return snapshot();
        }
        if (rule.shock_enabled) {
          const allIds = state.config.openshock_shockers.map((s) => s.id).filter(Boolean);
          const picked = rule.shock_shocker_ids == null ? allIds : rule.shock_shocker_ids;
          const ids = picked.filter((id) => allIds.includes(id));
          if (state.config.openshock_enabled && state.config.openshock_api_token && ids.length) {
            push(`test ${args.kind}: shock intensity=${rule.shock_intensity} duration_ms=${rule.shock_duration_ms} shockers=${ids.length}`);
          } else {
            push(`test ${args.kind}: shock trigger hit but openshock is not configured`);
          }
        }
        return snapshot();
      }
      case "connect_embedded":
        state.toy_mode = "Embedded";
        state.devices = ["Loonie (ex. Lovense)", "Edge"];
        state.toy_error = null;
        push("connected embedded: 1 device");
        return snapshot();
      case "connect_central":
        state.toy_mode = "ExternalCentral";
        state.devices = [];
        state.toy_error = null;
        push("connected central: no devices yet");
        return snapshot();
      case "disconnect":
        state.toy_mode = "Disconnected";
        state.devices = [];
        push("disconnected");
        return snapshot();
      case "rescan":
        push("rescan: 1 device found");
        return snapshot();
      default:
        throw new Error(`unknown command ${command}`);
    }
  };
})();
