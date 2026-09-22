import { getStatus, run } from "./backend.js";
import { els, h } from "./dom.js";

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

function status() {
  return getStatus();
}

function triggerLabel(kind) {
  const slot = kind.match(/^ability_(used|ready):(\d)$/);
  if (slot) {
    return `Ability ${Number(slot[2]) + 1} ${slot[1] === "used" ? "cast" : "ready"}`;
  }
  return TRIGGER_LABELS[kind] ?? kind;
}

function groupTriggers() {
  const buckets = TRIGGER_GROUPS.map((group) => ({ ...group, items: [] }));
  for (const trigger of status().triggers) {
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

function pressExclusive(group, btn) {
  for (const other of group.children) {
    other.setAttribute("aria-pressed", String(other === btn));
  }
}

function buildShockBox(trigger, row) {
  const box = h("div", { class: "shock-box" });

  const enabled = h("input", { type: "checkbox" });
  enabled.checked = Boolean(trigger.shock_enabled);

  const kind = buildSeg(SHOCK_KINDS, trigger.shock_kind ?? "shock", (value, btn) => {
    pressExclusive(kind, btn);
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
  const roster = status().config.openshock_shockers ?? [];
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
    pressExclusive(pattern, btn);
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
  const devices = status().devices;
  for (const device of devices) {
    vibrateRow.append(h("button", {
      class: "chip",
      type: "button",
      dataset: { device },
      "aria-pressed": String(vibSel.all || vibSel.ids.has(device)),
      onclick: () => {
        if (vibSel.all) {
          for (const other of devices) {
            if (other !== device) vibSel.ids.add(other);
          }
          vibSel.all = false;
        } else if (vibSel.ids.has(device)) {
          vibSel.ids.delete(device);
        } else {
          vibSel.ids.add(device);
        }
        if (devices.length > 0 && vibSel.ids.size === devices.length) {
          vibSel.all = true;
          vibSel.ids.clear();
        }
        syncVibrateChips();
        pushTrigger(trigger.kind, row);
      },
    }, device));
  }
  if (devices.length > 0) {
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
    vibrate_devices: vibSel.all
      ? [...status().devices]
      : [...vibSel.ids],
    shock_enabled: shock.enabled.checked,
    shock_intensity: Number(shock.intensity.value),
    shock_duration_ms: Number(shock.duration.value),
    shock_kind: pressedKind ? pressedKind.dataset.value : "shock",
    shock_shocker_ids: shock.sel.all
      ? (status().config.openshock_shockers ?? []).map((shocker) => shocker.id)
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

export function renderTriggers() {
  const groups = groupTriggers();
  const signature = JSON.stringify([
    groups.map((g) => [g.title, g.items.map((i) => i.kind)]),
    status().config.openshock_shockers ?? [],
    status().devices,
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
    els.triggerGroups.dataset.rev = String(status().config_rev);
    return;
  }
  if (els.triggerGroups.dataset.rev === String(status().config_rev)) return;
  els.triggerGroups.dataset.rev = String(status().config_rev);
  for (const row of els.triggerGroups.querySelectorAll(".trigger")) {
    if (row.contains(document.activeElement)) continue;
    const trigger = status().triggers.find((t) => t.kind === row.dataset.kind);
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
