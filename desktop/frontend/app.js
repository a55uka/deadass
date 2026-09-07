const tauri = window.__TAURI__ && window.__TAURI__.core;

const REFRESH_MS = 1000;

const elements = {
  badge: document.getElementById("trail-badge"),
  trailText: document.getElementById("trail-text"),
  logCard: document.getElementById("log-card"),
  logToggle: document.getElementById("btn-log-toggle"),
  logPath: document.getElementById("log-path"),
  log: document.getElementById("log"),
  toyMode: document.getElementById("toy-mode"),
  backendSelect: document.getElementById("toy-backend"),
  devices: document.getElementById("devices"),
  toyError: document.getElementById("toy-error"),
};

function trailState(phase, tailing) {
  const normalized = phase || (tailing ? "tailing" : "missing");
  return {
    waiting: normalized === "waiting",
    live: normalized === "tailing" || Boolean(tailing),
  };
}

function renderTrail(status) {
  const tailing = status.log_tailing ?? status.mod_active;
  const { waiting, live } = trailState(status.log_phase, tailing);
  elements.badge.classList.toggle("is-live", live && !waiting);
  elements.badge.classList.toggle("is-waiting", waiting);
  elements.badge.classList.toggle("is-off", !live && !waiting);
  elements.badge.classList.remove("is-unknown");
  elements.trailText.textContent = waiting
    ? "waiting for console.log"
    : live
      ? "trailing console.log"
      : "not trailing";
  elements.badge.title = trailDetail(status, waiting, live);
  elements.logPath.textContent = status.log_path || "";
}

function trailDetail(status, waiting, live) {
  if (waiting) {
    return status.log_path
      ? `Waiting for ${status.log_path}`
      : "Waiting for console.log to be created";
  }
  if (live) {
    return status.log_path ? `Tailing ${status.log_path}` : "Tailing console.log";
  }
  return status.log || "console.log not found";
}

function renderToys(status) {
  elements.toyMode.textContent = status.toy_mode;
  if (status.toy_mode === "Embedded") {
    elements.backendSelect.value = "embedded";
  } else if (status.toy_mode === "ExternalCentral") {
    elements.backendSelect.value = "central";
  }
  elements.devices.textContent =
    status.devices.length > 0 ? status.devices.join(", ") : "no devices";
  elements.toyError.hidden = !status.toy_error;
  elements.toyError.textContent = status.toy_error ? `toy error: ${status.toy_error}` : "";
}

function renderLog(status) {
  elements.log.textContent = (status.log_lines ?? [status.log]).join("\n");
  if (!elements.logCard.hidden) {
    elements.log.scrollTop = elements.log.scrollHeight;
  }
}

function renderBackendUnreachable(error) {
  renderTrail({ log_phase: "missing", log_tailing: false, log_path: null, log: String(error) });
  elements.log.textContent = `backend unreachable: ${error}`;
}

function setLogVisible(visible) {
  elements.logCard.hidden = !visible;
  elements.logToggle.textContent = visible ? "Hide log" : "Show log";
  elements.logToggle.setAttribute("aria-expanded", String(visible));
}

async function refresh() {
  try {
    const status = await tauri.invoke("get_status");
    renderTrail(status);
    renderToys(status);
    renderLog(status);
  } catch (error) {
    renderBackendUnreachable(error);
  }
}

async function run(command, args) {
  try {
    await tauri.invoke(command, args || {});
  } catch (error) {
    elements.log.textContent = `${command} failed: ${error}`;
  }
  refresh();
}

function bindControls() {
  elements.logToggle.addEventListener("click", () => {
    setLogVisible(elements.logCard.hidden);
  });
  document.getElementById("btn-connect").addEventListener("click", () => {
    const backend = elements.backendSelect.value;
    run(backend === "central" ? "connect_central" : "connect_embedded");
  });
  document.getElementById("btn-disconnect").addEventListener("click", () => run("disconnect"));
  document.getElementById("btn-rescan").addEventListener("click", () => run("rescan"));
  document.querySelectorAll("[data-fire]").forEach((button) => {
    button.addEventListener("click", () => run("test_fire", { kind: button.dataset.fire }));
  });
}

if (!tauri) {
  renderTrail({ log_phase: "missing", log_tailing: false, log_path: null });
  elements.log.textContent = "tauri bridge missing: rebuild with app.withGlobalTauri enabled";
} else {
  bindControls();
  setLogVisible(false);
  setInterval(refresh, REFRESH_MS);
  refresh();
}
