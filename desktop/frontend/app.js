const tauri = window.__TAURI__ && window.__TAURI__.core;
const REFRESH_MS = 1000;

const els = {
  nav: document.getElementById("nav"),
  dot: document.getElementById("rail-dot"),
  railLine1: document.getElementById("rail-line1"),
  railLine2: document.getElementById("rail-line2"),
  railDemo: document.getElementById("rail-demo"),
  stage: document.getElementById("stage"),
  commandError: document.getElementById("command-error"),
  sourceSeg: document.getElementById("source-seg"),
  sourcePill: document.getElementById("home-source-pill"),
  sourceDetail: document.getElementById("home-source-detail"),
  homeToys: document.getElementById("home-toys"),
  homeToyError: document.getElementById("home-toy-error"),
  homeLog: document.getElementById("home-log"),
  triggerGroups: document.getElementById("trigger-groups"),
  toyBackendSeg: document.getElementById("toy-backend-seg"),
  btnConnect: document.getElementById("btn-connect"),
  btnDisconnect: document.getElementById("btn-disconnect"),
  btnRescan: document.getElementById("btn-rescan"),
  devices: document.getElementById("devices"),
  devicesEmpty: document.getElementById("devices-empty"),
  toyError: document.getElementById("toy-error"),
  toysCount: document.getElementById("toys-count"),
  cfgGain: document.getElementById("cfg-master-gain"),
  cfgGainOut: document.getElementById("cfg-master-gain-out"),
  cfgCap: document.getElementById("cfg-cap"),
  cfgCapOut: document.getElementById("cfg-cap-out"),
  cfgMute: document.getElementById("cfg-mute"),
  cfgUrl: document.getElementById("cfg-central-url"),
  cfgDllPath: document.getElementById("cfg-dll-path"),
  cfgModPort: document.getElementById("cfg-mod-port"),
  cfgDllPort: document.getElementById("cfg-dll-port"),
  cfgDebug: document.getElementById("cfg-debug"),
  cfgOsEnabled: document.getElementById("cfg-openshock-enabled"),
  cfgOsToken: document.getElementById("cfg-openshock-token"),
  cfgOsBaseUrl: document.getElementById("cfg-openshock-base-url"),
  cfgOsState: document.getElementById("cfg-openshock-state"),
  cfgOsHint: document.getElementById("cfg-openshock-hint"),
  osRoster: document.getElementById("openshock-roster"),
  btnOsAdd: document.getElementById("btn-openshock-add"),
  btnOsSave: document.getElementById("btn-openshock-save"),
  btnOsTest: document.getElementById("btn-openshock-test"),
  btnToysTest: document.getElementById("btn-toys-test"),
  configPath: document.getElementById("config-path"),
  log: document.getElementById("log"),
  btnLogCopy: document.getElementById("btn-log-copy"),
  btnLogBottom: document.getElementById("btn-log-bottom"),
};

const TRIGGER_GROUPS = [
  { title: "Combat", kinds: ["kill", "death", "assist", "respawn"] },
  { title: "Melee", kinds: ["parry", "parried", "punch_landed", "punch_taken"] },
  { title: "Ability cast", match: /^ability_used:(\d)$/ },
  { title: "Ability ready", match: /^ability_ready:(\d)$/ },
];

const TRIGGER_LABELS = {
  kill: "Kill",
  death: "Death",
  assist: "Assist",
  respawn: "Respawn",
  parry: "Parry landed",
  parried: "Got parried",
  punch_landed: "Punch landed",
  punch_taken: "Punch taken",
};

const PATTERNS = ["vibrate", "pulse", "ramp"];
const SHOCK_KINDS = ["shock", "vibrate", "sound"];

let status = null;
let backendError = null;
let currentView = "home";
// Controls with an in-flight invoke; the renderer leaves them alone so the
// server echo cannot fight the user's hand.
const pending = new Set();

/* ---------- tiny helpers ---------- */

function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value == null || value === false) continue;
    if (key === "class") el.className = value;
    else if (key === "dataset") Object.assign(el.dataset, value);
    else if (key.startsWith("on")) el.addEventListener(key.slice(2), value);
    else el.setAttribute(key, value === true ? "" : value);
  }
  for (const child of children.flat()) {
    if (child == null) continue;
    el.append(child.nodeType ? child : document.createTextNode(child));
  }
  return el;
}

function setValue(control, apply) {
  if (pending.has(control) || control === document.activeElement) return;
  apply();
}

function triggerLabel(kind) {
  const slot = kind.match(/^ability_(used|ready):(\d)$/);
  if (slot) {
    return `Ability ${Number(slot[2]) + 1} ${slot[1] === "used" ? "cast" : "ready"}`;
  }
  return TRIGGER_LABELS[kind] ?? kind;
}

function logLineClass(line) {
  if (/^vibrate /.test(line) || /^connected/.test(line)) return "is-fire";
  if (/failed|error|could not/i.test(line)) return "is-error";
  if (/^suppress |waiting|missing/i.test(line)) return "is-dim";
  return "";
}

/* ---------- backend bridge (demo.js installs __demoBridge when the Tauri
   bridge is missing, e.g. when previewing the UI in a plain browser) ---------- */

const invoke = window.__demoBridge ?? ((command, args) => tauri.invoke(command, args));

/* ---------- render: rail ---------- */

function trailInfo() {
  const waiting = status.log_phase === "waiting";
  const live = status.log_phase === "tailing" || status.mod_active;
  const dllSelected = status.data_source === "dll";
  const injected = status.inject_phase === "injected";
  let phase, text;
  if (dllSelected) {
    phase = injected ? "live" : "off";
    text = injected
      ? status.dll_active ? "DLL injected, events live" : "DLL injected"
      : `DLL ${status.inject_phase}`;
  } else {
    phase = waiting ? "waiting" : live ? "live" : "off";
    text = waiting ? "Waiting for console.log" : live ? "Tailing console.log" : "Console.log missing";
  }
  return { phase, text, dllSelected };
}

function toyLine() {
  switch (status.toy_mode) {
    case "Embedded": return `Embedded, ${status.devices.length} device${status.devices.length === 1 ? "" : "s"}`;
    case "ExternalCentral": return `Intiface Central, ${status.devices.length} device${status.devices.length === 1 ? "" : "s"}`;
    default: return "Not connected";
  }
}

function renderRail() {
  const { phase, text } = trailInfo();
  els.dot.className = `status-dot is-${phase}`;
  els.dot.title = text;
  els.railLine1.textContent = text;
  els.railLine2.textContent = toyLine();
}

let lastFiredLine = null;

function flashOnFire() {
  const last = status.log_lines.at(-1) ?? "";
  if (!/^vibrate /.test(last) || last === lastFiredLine) return;
  lastFiredLine = last;
  els.dot.classList.add("is-firing");
  setTimeout(() => els.dot.classList.remove("is-firing"), 450);
}

/* ---------- render: home ---------- */

function renderHome() {
  const source = status.data_source;
  els.sourcePill.textContent = source;
  for (const btn of els.sourceSeg.children) {
    btn.setAttribute("aria-pressed", String(btn.dataset.source === source));
  }
  let detail;
  if (source === "dll") {
    detail = `Injection ${status.inject_phase}.`;
    if (status.inject_detail) detail += ` ${status.inject_detail}`;
    detail += status.dll_active ? " Receiving events." : " No events yet.";
  } else if (status.log_phase === "waiting") {
    detail = `Waiting for ${status.log_path ?? "console.log"}.`;
  } else if (status.log_phase === "tailing") {
    detail = `Tailing ${status.log_path ?? "console.log"}.`;
  } else {
    detail = status.log || "console.log not found";
  }
  els.sourceDetail.textContent = detail;
  els.homeToys.textContent = `${toyLine()}.`;
  els.homeToyError.hidden = !status.toy_error;
  els.homeToyError.textContent = status.toy_error ?? "";
  renderLogInto(els.homeLog, status.log_lines.slice(-6));
}

/* ---------- render: triggers ---------- */

function groupTriggers() {
  const buckets = TRIGGER_GROUPS.map((group) => ({ ...group, items: [] }));
  for (const trigger of status.triggers) {
    for (const bucket of buckets) {
      if (bucket.kinds?.includes(trigger.kind) || bucket.match?.test(trigger.kind)) {
        bucket.items.push(trigger);
        break;
      }
    }
  }
  return buckets.filter((bucket) => bucket.items.length > 0);
}

function buildSeg(options, pressed, onSelect, small) {
  const seg = h("div", { class: "seg", role: "group" });
  for (const option of options) {
    seg.append(h("button", {
      class: "seg-btn",
      type: "button",
      dataset: { value: option },
      "aria-pressed": String(option === pressed),
      style: small ? "padding:3px 10px;font-size:12.5px" : null,
      onclick: (event) => onSelect(option, event.currentTarget),
    }, option));
  }
  return seg;
}

function buildShockBox(trigger, row) {
  const box = h("div", { class: "shock-box" });

  const enabled = h("input", { type: "checkbox" });
  enabled.checked = Boolean(trigger.shock_enabled);

  const kind = buildSeg(SHOCK_KINDS, trigger.shock_kind ?? "shock", (value, btn) => {
    for (const other of btn.parentElement.children) {
      other.setAttribute("aria-pressed", String(other === btn));
    }
    pushTrigger(trigger.kind, row);
  }, true);

  const intensity = h("input", { type: "range", min: "1", max: "100", step: "1" });
  const intensityOut = h("output", { class: "mono field-value" });
  intensity.value = String(trigger.shock_intensity ?? 20);
  intensityOut.textContent = `${intensity.value}%`;
  intensity.addEventListener("input", () => {
    intensityOut.textContent = `${intensity.value}%`;
  });

  const duration = h("input", { type: "number", min: "300", max: "30000", step: "100" });
  duration.value = String(trigger.shock_duration_ms ?? 300);

  // sel.all means the selection was never touched: every roster shocker.
  const sel = {
    all: trigger.shock_shocker_ids == null,
    ids: new Set(trigger.shock_shocker_ids ?? []),
  };
  const chipRow = h("div", { class: "chip-row" }, h("span", { class: "field-label" }, "Shockers"));
  const syncShockChips = () => {
    for (const btn of chipRow.querySelectorAll(".chip")) {
      btn.setAttribute("aria-pressed", String(sel.all || sel.ids.has(btn.title)));
    }
  };
  const roster = status.config.openshock_shockers ?? [];
  if (!roster.length) {
    chipRow.append(h("a", { class: "muted shock-note", href: "#settings" }, "No shockers yet — add one in Settings."));
  } else {
    const rosterIds = roster.map((shocker) => shocker.id);
    for (const shocker of roster) {
      chipRow.append(h("button", {
        class: "chip",
        type: "button",
        title: shocker.id,
        "aria-pressed": String(sel.all || sel.ids.has(shocker.id)),
        onclick: () => {
          if (sel.all) {
            for (const id of rosterIds) {
              if (id !== shocker.id) sel.ids.add(id);
            }
            sel.all = false;
          } else if (sel.ids.has(shocker.id)) {
            sel.ids.delete(shocker.id);
          } else {
            sel.ids.add(shocker.id);
          }
          if (sel.ids.size === rosterIds.length) {
            // fully selected again: fold back to the untouched "all" state
            sel.all = true;
            sel.ids.clear();
          }
          syncShockChips();
          pushTrigger(trigger.kind, row);
        },
      }, shocker.name || shocker.id));
    }
  }

  box.append(
    h("div", { class: "shock-head" },
      h("span", { class: "shock-title" }, "Shock"),
      kind,
      h("label", { class: "switch" }, enabled, h("span", { class: "track" })),
    ),
    h("div", { class: "trigger-controls" },
      h("label", { class: "field field-grow" },
        h("span", { class: "field-label" }, "Intensity"), intensity),
      intensityOut,
      h("label", { class: "field field-nums" },
        h("span", { class: "field-label" }, "Duration ms"), duration),
    ),
    chipRow,
  );

  box.classList.toggle("is-off", !trigger.shock_enabled);
  box._controls = { enabled, intensity, intensityOut, duration, kind, sel };
  for (const control of [enabled, duration]) {
    control.addEventListener("change", () => pushTrigger(trigger.kind, row));
  }
  intensity.addEventListener("change", () => pushTrigger(trigger.kind, row));
  return box;
}

function buildTriggerRow(trigger) {
  const row = h("article", { class: "trigger", dataset: { kind: trigger.kind } });

  const enabled = h("input", { type: "checkbox" });
  enabled.checked = trigger.enabled;
  const test = h("button", { class: "btn trigger-test", type: "button", onclick: () => run("test_fire", { kind: trigger.kind }, [test]) }, "Test");
  row.append(h("div", { class: "trigger-head" },
    h("span", { class: "trigger-name" }, triggerLabel(trigger.kind)),
    test,
    h("label", { class: "switch" }, enabled, h("span", { class: "track" })),
  ));

  const strength = h("input", { type: "range", min: "0", max: "1", step: "0.05" });
  const strengthOut = h("output", { class: "mono field-value" });
  strength.value = String(trigger.strength);
  strengthOut.textContent = `${Math.round(trigger.strength * 100)}%`;
  strength.addEventListener("input", () => {
    strengthOut.textContent = `${Math.round(strength.value * 100)}%`;
  });

  const duration = h("input", { type: "number", min: "50", max: "5000", step: "50" });
  duration.value = String(trigger.duration_ms);
  const cooldown = h("input", { type: "number", min: "0", max: "10000", step: "50" });
  cooldown.value = String(trigger.retrigger_cooldown_ms);

  const pattern = buildSeg(PATTERNS, trigger.pattern, (value, btn) => {
    for (const other of btn.parentElement.children) {
      other.setAttribute("aria-pressed", String(other === btn));
    }
    pushTrigger(trigger.kind, row);
  });

  row.append(h("div", { class: "trigger-controls" },
    h("label", { class: "field field-grow" },
      h("span", { class: "field-label" }, "Strength"), strength),
    strengthOut,
    h("label", { class: "field field-nums" },
      h("span", { class: "field-label" }, "Duration ms"), duration),
    h("label", { class: "field field-nums" },
      h("span", { class: "field-label" }, "Cooldown ms"), cooldown),
    h("div", { class: "field" },
      h("span", { class: "field-label" }, "Pattern"), pattern),
  ));

  const shockBox = buildShockBox(trigger, row);
  row.append(shockBox);

  // vibSel.all means the selection was never touched: every connected toy.
  const vibSel = {
    all: trigger.vibrate_devices == null,
    ids: new Set(trigger.vibrate_devices ?? []),
  };
  const vibrateRow = h("div", { class: "chip-row" }, h("span", { class: "field-label" }, "Vibrates"));
  const syncVibrateChips = () => {
    for (const btn of vibrateRow.querySelectorAll(".chip")) {
      btn.setAttribute("aria-pressed", String(vibSel.all || vibSel.ids.has(btn.dataset.device)));
    }
  };
  for (const device of status.devices) {
    vibrateRow.append(h("button", {
      class: "chip",
      type: "button",
      dataset: { device },
      "aria-pressed": String(vibSel.all || vibSel.ids.has(device)),
      onclick: () => {
        if (vibSel.all) {
          for (const other of status.devices) {
            if (other !== device) vibSel.ids.add(other);
          }
          vibSel.all = false;
        } else if (vibSel.ids.has(device)) {
          vibSel.ids.delete(device);
        } else {
          vibSel.ids.add(device);
        }
        if (status.devices.length > 0 && vibSel.ids.size === status.devices.length) {
          // fully selected again: fold back to the untouched "all" state
          vibSel.all = true;
          vibSel.ids.clear();
        }
        syncVibrateChips();
        pushTrigger(trigger.kind, row);
      },
    }, device));
  }
  if (status.devices.length > 0) {
    row.insertBefore(vibrateRow, shockBox);
  }

  row._controls = {
    enabled, strength, strengthOut, duration, cooldown, pattern,
    vibSel, syncVibrateChips,
    shock: shockBox._controls, shockBox,
  };
  for (const control of [enabled, duration, cooldown]) {
    control.addEventListener("change", () => pushTrigger(trigger.kind, row));
  }
  strength.addEventListener("change", () => pushTrigger(trigger.kind, row));
  return row;
}

function readTriggerRow(kind, row) {
  const { enabled, strength, duration, cooldown, pattern, vibSel, shock } = row._controls;
  const pressed = [...pattern.children].find((btn) => btn.getAttribute("aria-pressed") === "true");
  const pressedKind = [...shock.kind.children].find((btn) => btn.getAttribute("aria-pressed") === "true");
  return {
    kind,
    enabled: enabled.checked,
    strength: Number(strength.value),
    duration_ms: Number(duration.value),
    retrigger_cooldown_ms: Number(cooldown.value),
    pattern: pressed ? pressed.dataset.value : "vibrate",
    // Always send explicit lists: "all" is materialized so the backend never
    // has to guess. Untouched configs keep null in the file (= all devices).
    vibrate_devices: vibSel.all
      ? [...status.devices]
      : [...vibSel.ids],
    shock_enabled: shock.enabled.checked,
    shock_intensity: Number(shock.intensity.value),
    shock_duration_ms: Number(shock.duration.value),
    shock_kind: pressedKind ? pressedKind.dataset.value : "shock",
    shock_shocker_ids: shock.sel.all
      ? (status.config.openshock_shockers ?? []).map((shocker) => shocker.id)
      : [...shock.sel.ids],
  };
}

function pushTrigger(kind, row) {
  const payload = readTriggerRow(kind, row);
  const shock = row._controls.shock;
  const controls = [
    row._controls.enabled, row._controls.strength, row._controls.duration, row._controls.cooldown,
    shock.enabled, shock.intensity, shock.duration,
  ];
  run("set_trigger", payload, controls);
}

let triggerSignature = null;

function renderTriggers() {
  const groups = groupTriggers();
  // Roster and connected toys are structural: trigger rows render one chip
  // per shocker and per toy, so either changing rebuilds the rows.
  const signature = JSON.stringify([
    groups.map((g) => [g.title, g.items.map((i) => i.kind)]),
    status.config.openshock_shockers ?? [],
    status.devices,
  ]);
  if (signature !== triggerSignature) {
    triggerSignature = signature;
    els.triggerGroups.replaceChildren();
    for (const group of groups) {
      const list = h("div", { class: "trigger-list" });
      for (const trigger of group.items) {
        const row = buildTriggerRow(trigger);
        list.append(row);
      }
      els.triggerGroups.append(h("div", { class: "group" },
        h("div", { class: "group-head" }, h("h2", {}, group.title)), list));
    }
    els.triggerGroups.dataset.rev = String(status.config_rev);
    return;
  }
  if (els.triggerGroups.dataset.rev === String(status.config_rev)) return;
  els.triggerGroups.dataset.rev = String(status.config_rev);
  for (const row of els.triggerGroups.querySelectorAll(".trigger")) {
    if (row.contains(document.activeElement)) continue;
    const trigger = status.triggers.find((t) => t.kind === row.dataset.kind);
    if (!trigger) continue;
    const { enabled, strength, strengthOut, duration, cooldown, pattern, vibSel, syncVibrateChips, shock, shockBox } =
      row._controls;
    enabled.checked = trigger.enabled;
    strength.value = String(trigger.strength);
    strengthOut.textContent = `${Math.round(trigger.strength * 100)}%`;
    duration.value = String(trigger.duration_ms);
    cooldown.value = String(trigger.retrigger_cooldown_ms);
    vibSel.all = trigger.vibrate_devices == null;
    vibSel.ids.clear();
    for (const name of trigger.vibrate_devices ?? []) vibSel.ids.add(name);
    syncVibrateChips();
    shock.enabled.checked = Boolean(trigger.shock_enabled);
    shock.intensity.value = String(trigger.shock_intensity ?? 20);
    shock.intensityOut.textContent = `${shock.intensity.value}%`;
    shock.duration.value = String(trigger.shock_duration_ms ?? 300);
    // Mutate the set in place: the chip handlers hold a reference to it.
    shock.sel.all = trigger.shock_shocker_ids == null;
    shock.sel.ids.clear();
    for (const id of trigger.shock_shocker_ids ?? []) shock.sel.ids.add(id);
    for (const btn of pattern.children) {
      btn.setAttribute("aria-pressed", String(btn.dataset.value === trigger.pattern));
    }
    for (const btn of shock.kind.children) {
      btn.setAttribute("aria-pressed", String(btn.dataset.value === (trigger.shock_kind ?? "shock")));
    }
    for (const chip of shockBox.querySelectorAll(".chip")) {
      chip.setAttribute("aria-pressed", String(shock.sel.all || shock.sel.ids.has(chip.title)));
    }
    shockBox.classList.toggle("is-off", !trigger.shock_enabled);
  }
}

/* ---------- render: toys ---------- */

let chosenBackend = "embedded";

function renderToys() {
  const mode = status.toy_mode;
  if (mode === "Embedded") chosenBackend = "embedded";
  else if (mode === "ExternalCentral") chosenBackend = "central";
  for (const btn of els.toyBackendSeg.children) {
    btn.setAttribute("aria-pressed", String(btn.dataset.backend === chosenBackend));
  }
  els.toysCount.textContent = String(status.devices.length);
  els.devices.replaceChildren(...status.devices.map((name) => h("li", {},
    h("span", { class: "status-dot is-live", "aria-hidden": "true" }), name)));
  els.devicesEmpty.hidden = status.devices.length > 0;
  els.toyError.hidden = !status.toy_error;
  els.toyError.textContent = status.toy_error ? `Connect failed: ${status.toy_error}` : "";
}

/* ---------- render: settings / shocker roster ---------- */

// `roster` is the single source the rows render from. While clean it tracks
// the saved config (so file reloads show up); the first edit marks it dirty
// and subsequent polls leave it alone until Save.
let roster = [];
let rosterDirty = false;
const OS_HINT_DEFAULT = "Shocker IDs are GUIDs — in the OpenShock app, open the shocker and copy its ID. Pick which shockers each trigger fires in Triggers.";

function savedRoster() {
  return status?.config.openshock_shockers ?? [];
}

function osStateText() {
  const cfg = status.config;
  if (!cfg.openshock_enabled) return "off";
  if (cfg.openshock_ready) {
    const count = cfg.openshock_shockers.length;
    return `ready, ${count} shocker${count === 1 ? "" : "s"}`;
  }
  return "not configured";
}

function renderRoster() {
  if (rosterDirty) return;
  const signature = JSON.stringify(savedRoster());
  if (els.osRoster.dataset.signature === signature) return;
  els.osRoster.dataset.signature = signature;
  roster = savedRoster().map((entry) => ({ ...entry }));
  drawRoster();
}

function drawRoster() {
  els.osRoster.replaceChildren();
  if (!roster.length) {
    els.osRoster.append(h("p", { class: "roster-empty" },
      "No shockers yet. Add one and paste its ID from OpenShock."));
    return;
  }
  for (const entry of roster) els.osRoster.append(rosterRow(entry));
}

function looksLikeGuid(id) {
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(id.trim());
}

function rosterRow(entry) {
  const nameInput = h("input", { type: "text", value: entry.name, placeholder: "name" });
  const idInput = h("input", {
    type: "text", class: "mono-input roster-id", value: entry.id,
    placeholder: "shocker ID", spellcheck: "false",
  });
  const idWarning = h("span", { class: "roster-warn", hidden: true }, "not a GUID");
  const syncWarning = () => {
    idWarning.hidden = !(entry.id.trim() && !looksLikeGuid(entry.id));
  };
  syncWarning();
  nameInput.addEventListener("input", () => { entry.name = nameInput.value; markRosterDirty(); });
  idInput.addEventListener("input", () => {
    entry.id = idInput.value;
    syncWarning();
    markRosterDirty();
  });
  return h("div", { class: "roster-row" }, nameInput, idInput, idWarning,
    h("button", {
      class: "roster-remove", type: "button", title: "Remove shocker",
      onclick: () => {
        roster.splice(roster.indexOf(entry), 1);
        markRosterDirty();
        drawRoster();
      },
    }, "Remove"));
}

function markRosterDirty() {
  rosterDirty = true;
  refreshOsHint();
}

function refreshOsHint() {
  els.cfgOsHint.textContent = rosterDirty
    ? "Unsaved shocker changes — use Save shockers."
    : OS_HINT_DEFAULT;
}

/* ---------- render: config ---------- */

function renderConfig() {
  const cfg = status.config;
  setValue(els.cfgGain, () => {
    els.cfgGain.value = String(cfg.master_gain);
    els.cfgGainOut.textContent = `×${cfg.master_gain.toFixed(2)}`;
  });
  setValue(els.cfgCap, () => {
    els.cfgCap.value = String(cfg.max_strength_cap);
    els.cfgCapOut.textContent = `${Math.round(cfg.max_strength_cap * 100)}%`;
  });
  setValue(els.cfgMute, () => { els.cfgMute.checked = cfg.mute_while_dead; });
  setValue(els.cfgDebug, () => { els.cfgDebug.checked = cfg.debug_logging; });
  setValue(els.cfgUrl, () => { els.cfgUrl.value = cfg.buttplug_ws_url; });
  setValue(els.cfgDllPath, () => { els.cfgDllPath.value = cfg.dll_path ?? ""; });
  setValue(els.cfgOsEnabled, () => { els.cfgOsEnabled.checked = cfg.openshock_enabled; });
  setValue(els.cfgOsToken, () => { els.cfgOsToken.value = cfg.openshock_api_token; });
  setValue(els.cfgOsBaseUrl, () => { els.cfgOsBaseUrl.value = cfg.openshock_base_url; });
  els.cfgOsState.textContent = osStateText();
  renderRoster();
  refreshOsHint();
  els.cfgModPort.textContent = String(cfg.mod_http_port);
  els.cfgDllPort.textContent = String(cfg.dll_event_port);
  els.configPath.textContent = status.config_path;
}

/* ---------- render: logs ---------- */

function renderLogInto(el, lines) {
  el.replaceChildren(...lines.map((line) => h("span", { class: `log-line ${logLineClass(line)}` }, line + "\n")));
}

let logSignature = null;
let logStick = true;

function renderLogs() {
  const lines = status.log_lines;
  const signature = `${lines.length}|${lines.at(-1) ?? ""}`;
  if (signature !== logSignature) {
    logSignature = signature;
    renderLogInto(els.log, lines);
    if (logStick) els.log.scrollTop = els.log.scrollHeight;
    els.btnLogBottom.hidden = logStick;
  }
}

/* ---------- commands ---------- */

async function run(command, args = {}, controls = []) {
  for (const control of controls) {
    pending.add(control);
    disable(control, true);
  }
  try {
    status = await invoke(command, args);
    showCommandError(null);
    return true;
  } catch (error) {
    showCommandError(`${command} failed: ${error}`);
    return false;
  } finally {
    for (const control of controls) {
      pending.delete(control);
      disable(control, false);
    }
    renderAll();
  }
}

function disable(control, value) {
  if ("disabled" in control) control.disabled = value;
}

function showCommandError(message) {
  els.commandError.hidden = !message;
  els.commandError.replaceChildren();
  if (message) {
    els.commandError.append(h("span", {}, message), h("button", { type: "button", onclick: () => showCommandError(null) }, "Dismiss"));
  }
}

async function poll() {
  try {
    status = await invoke("get_status", {});
    backendError = null;
    renderAll();
  } catch (error) {
    backendError = String(error);
    showCommandError(`Backend unreachable: ${error}`);
    els.dot.className = "status-dot is-off";
  }
}

function renderAll() {
  if (!status) return;
  renderRail();
  renderHome();
  renderTriggers();
  renderToys();
  renderConfig();
  renderLogs();
  flashOnFire();
}

/* ---------- router ---------- */

const VIEWS = [...document.querySelectorAll(".view")].map((panel) => panel.dataset.panel);

function showView(view) {
  if (!VIEWS.includes(view)) {
    view = "home";
  }
  currentView = view;
  for (const panel of document.querySelectorAll(".view")) {
    panel.hidden = panel.dataset.panel !== view;
  }
  for (const item of els.nav.querySelectorAll(".nav-item")) {
    if (item.getAttribute("href") === `#${view}`) item.setAttribute("aria-current", "page");
    else item.removeAttribute("aria-current");
  }
  els.stage.scrollTop = 0;
  if (location.hash !== `#${view}`) location.hash = view;
}

function routeFromHash() {
  const raw = location.hash.slice(1) || "home";
  // Older builds called the settings view "config".
  showView(raw === "config" ? "settings" : raw);
}

/* ---------- bindings ---------- */

function bindRange(control, output, format, key) {
  control.addEventListener("input", () => {
    output.textContent = format(Number(control.value));
  });
  control.addEventListener("change", () => {
    run("set_config", { [key]: Number(control.value) }, [control]);
  });
}

function bindStatic() {
  for (const btn of els.sourceSeg.children) {
    btn.addEventListener("click", () => run("set_data_source", { source: btn.dataset.source }, [btn]));
  }
  for (const btn of els.toyBackendSeg.children) {
    btn.addEventListener("click", () => {
      chosenBackend = btn.dataset.backend;
      renderToys();
    });
  }
  els.btnConnect.addEventListener("click", () => {
    run(chosenBackend === "central" ? "connect_central" : "connect_embedded", {}, [els.btnConnect]);
  });
  els.btnDisconnect.addEventListener("click", () => run("disconnect", {}, [els.btnDisconnect]));
  els.btnRescan.addEventListener("click", () => run("rescan", {}, [els.btnRescan]));

  bindRange(els.cfgGain, els.cfgGainOut, (v) => `×${v.toFixed(2)}`, "master_gain");
  bindRange(els.cfgCap, els.cfgCapOut, (v) => `${Math.round(v * 100)}%`, "max_strength_cap");
  els.cfgMute.addEventListener("change", () => run("set_config", { mute_while_dead: els.cfgMute.checked }, [els.cfgMute]));
  els.cfgDebug.addEventListener("change", () => run("set_config", { debug_logging: els.cfgDebug.checked }, [els.cfgDebug]));
  els.cfgUrl.addEventListener("change", () => run("set_config", { buttplug_ws_url: els.cfgUrl.value }, [els.cfgUrl]));
  els.cfgDllPath.addEventListener("change", () => run("set_config", { dll_path: els.cfgDllPath.value }, [els.cfgDllPath]));
  els.cfgOsEnabled.addEventListener("change", () => run("set_config", { openshock_enabled: els.cfgOsEnabled.checked }, [els.cfgOsEnabled]));
  els.cfgOsToken.addEventListener("change", () => run("set_config", { openshock_api_token: els.cfgOsToken.value }, [els.cfgOsToken]));
  els.cfgOsBaseUrl.addEventListener("change", () => run("set_config", { openshock_base_url: els.cfgOsBaseUrl.value }, [els.cfgOsBaseUrl]));
  els.btnOsAdd.addEventListener("click", () => {
    roster.push({ id: "", name: "" });
    markRosterDirty();
    drawRoster();
    els.osRoster.querySelector(".roster-row:last-child input")?.focus();
  });
  els.btnOsSave.addEventListener("click", async () => {
    const saved = await run("set_config", { openshock_shockers: roster }, [els.btnOsSave]);
    if (!saved) return;
    rosterDirty = false;
    delete els.osRoster.dataset.signature;
    renderRoster();
    refreshOsHint();
  });
  els.btnOsTest.addEventListener("click", () => run("test_openshock", {}, [els.btnOsTest]));
  els.btnToysTest.addEventListener("click", () => run("test_toys", {}, [els.btnToysTest]));

  els.log.addEventListener("scroll", () => {
    logStick = els.log.scrollTop + els.log.clientHeight >= els.log.scrollHeight - 40;
    els.btnLogBottom.hidden = logStick;
  });
  els.btnLogBottom.addEventListener("click", () => {
    logStick = true;
    els.log.scrollTop = els.log.scrollHeight;
    els.btnLogBottom.hidden = true;
  });
  els.btnLogCopy.addEventListener("click", async () => {
    if (!status) return;
    try {
      await navigator.clipboard.writeText(status.log_lines.join("\n"));
      els.btnLogCopy.textContent = "Copied";
      setTimeout(() => { els.btnLogCopy.textContent = "Copy"; }, 1200);
    } catch {
      showCommandError("Copy failed: the clipboard refused access");
    }
  });

  for (const btn of document.querySelectorAll("[data-fire]")) {
    btn.addEventListener("click", () => run("test_fire", { kind: btn.dataset.fire }, [btn]));
  }

  window.addEventListener("hashchange", routeFromHash);
}

/* ---------- boot ---------- */

if (!tauri) {
  els.railDemo.hidden = false;
  els.railLine1.textContent = "preview data";
}

bindStatic();
routeFromHash();
setInterval(poll, REFRESH_MS);
poll();
