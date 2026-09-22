import { getStatus, run } from "./backend.js";
import { els, h, setValue } from "./dom.js";

const OS_HINT_DEFAULT = "Shocker IDs are GUIDs — in the OpenShock app, open the shocker and copy its ID. Pick which shockers each trigger fires in Triggers.";

let roster = [];
let rosterDirty = false;

function status() {
  return getStatus();
}

function savedRoster() {
  return status()?.config.openshock_shockers ?? [];
}

function osStateText() {
  const cfg = status().config;
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

export function renderConfig() {
  const cfg = status().config;
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
  els.configPath.textContent = status().config_path;
}

export function renderUpdates() {
  const update = status()?.update ?? {};
  const available = update.available;
  const latest = update.latest_version;

  els.updCurrent.textContent = `v${status()?.app_version ?? "?"}`;
  els.updLatest.textContent = latest ? `v${latest}` : "not checked";
  els.updStatus.textContent = available
    ? `update available: v${available}`
    : latest
      ? "up to date"
      : "not checked";

  const lines = [];
  if (available) {
    lines.push(`Update available: v${available} — restart deadass to apply a staged update.`);
  } else if (latest) {
    lines.push(`Up to date (v${status()?.app_version ?? "?"}).`);
  }
  els.homeUpdate.hidden = lines.length === 0;
  els.homeUpdateText.textContent = lines.join(" ");
}

function bindRange(control, output, format, key) {
  control.addEventListener("input", () => {
    output.textContent = format(Number(control.value));
  });
  control.addEventListener("change", () => {
    run("set_config", { [key]: Number(control.value) }, [control]);
  });
}

export function bindSettings() {
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

  els.btnUpdCheck.addEventListener("click", () => run("check_updates", {}, [els.btnUpdCheck]));
  els.btnUpdOffsets.addEventListener("click", () => run("update_offsets_now", {}, [els.btnUpdOffsets]));
  els.btnUpdApp.addEventListener("click", () => run("update_app_now", {}, [els.btnUpdApp]));
}
