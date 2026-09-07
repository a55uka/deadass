const tauriCore = window.__TAURI__ && window.__TAURI__.core;

const badge = document.getElementById("trail-badge");
const trailText = document.getElementById("trail-text");
const logCard = document.getElementById("log-card");
const logToggle = document.getElementById("btn-log-toggle");
const logPath = document.getElementById("log-path");

function setTrail(phase, tailing, path, log) {
  const normalized = phase || (tailing ? "tailing" : "missing");
  const waiting = normalized === "waiting";
  const live = normalized === "tailing" || Boolean(tailing);
  badge.classList.toggle("is-live", live && !waiting);
  badge.classList.toggle("is-waiting", waiting);
  badge.classList.toggle("is-off", !live && !waiting);
  badge.classList.remove("is-unknown");
  trailText.textContent = waiting
    ? "waiting for console.log"
    : live
      ? "trailing console.log"
      : "not trailing";
  const detail = waiting
    ? (path ? `Waiting for ${path}` : "Waiting for console.log to be created")
    : live
      ? (path ? `Tailing ${path}` : "Tailing console.log")
      : (log || "console.log not found");
  badge.title = detail;
  if (logPath) {
    logPath.textContent = path || "";
  }
}

function setLogVisible(visible) {
  logCard.hidden = !visible;
  logToggle.textContent = visible ? "Hide log" : "Show log";
  logToggle.setAttribute("aria-expanded", String(visible));
}

logToggle.addEventListener("click", () => {
  setLogVisible(logCard.hidden);
});

async function refresh() {
  try {
    const status = await tauriCore.invoke("get_status");
    // New explicit phase; fall back to legacy flags for old backends.
    const tailing = status.log_tailing ?? status.mod_active;
    setTrail(status.log_phase, tailing, status.log_path, status.log);
    const mode = document.getElementById("toy-mode");
    mode.textContent = status.toy_mode;
    const backendSelect = document.getElementById("toy-backend");
    if (status.toy_mode === "Embedded") {
      backendSelect.value = "embedded";
    } else if (status.toy_mode === "ExternalCentral") {
      backendSelect.value = "central";
    }
    document.getElementById("devices").textContent =
      status.devices.length > 0 ? status.devices.join(", ") : "no devices";
    const error = document.getElementById("toy-error");
    error.hidden = !status.toy_error;
    error.textContent = status.toy_error ? `toy error: ${status.toy_error}` : "";
    const logEl = document.getElementById("log");
    logEl.textContent = (status.log_lines ?? [status.log]).join("\n");
    if (!logCard.hidden) {
      logEl.scrollTop = logEl.scrollHeight;
    }
  } catch (err) {
    setTrail("missing", false, null, String(err));
    document.getElementById("log").textContent = `backend unreachable: ${err}`;
  }
}

async function run(command, args) {
  try {
    await tauriCore.invoke(command, args || {});
  } catch (err) {
    document.getElementById("log").textContent = `${command} failed: ${err}`;
  }
  refresh();
}

document.getElementById("btn-connect").addEventListener("click", () => {
  const backend = document.getElementById("toy-backend").value;
  run(backend === "central" ? "connect_central" : "connect_embedded");
});
document.getElementById("btn-disconnect").addEventListener("click", () => run("disconnect"));
document.getElementById("btn-rescan").addEventListener("click", () => run("rescan"));
document.querySelectorAll("[data-fire]").forEach((button) => {
  button.addEventListener("click", () => run("test_fire", { kind: button.dataset.fire }));
});

const logLine = document.getElementById("log");

if (!tauriCore) {
  setTrail("missing", false, null, "tauri bridge missing");
  logLine.textContent =
    "tauri bridge missing: rebuild with app.withGlobalTauri enabled";
} else {
  setLogVisible(false);
  setInterval(refresh, 1000);
  refresh();
}
