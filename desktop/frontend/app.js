const tauriCore = window.__TAURI__ && window.__TAURI__.core;

function setDot(id, active) {
  const dot = document.getElementById(id);
  dot.textContent = active ? "●" : "○";
  dot.classList.toggle("on", active);
}

async function refresh() {
  try {
    const status = await tauriCore.invoke("get_status");
    setDot("dot-mod", status.mod_active);
    setDot("dot-dll", status.dll_active);
    setDot("dot-external", status.external_active);
    document.getElementById("toy-mode").textContent = status.toy_mode;
    document.getElementById("devices").textContent =
      status.devices.length > 0 ? status.devices.join(", ") : "no devices";
    const error = document.getElementById("toy-error");
    error.hidden = !status.toy_error;
    error.textContent = status.toy_error ? `toy error: ${status.toy_error}` : "";
    document.getElementById("log").textContent = status.log;
  } catch (err) {
    document.getElementById("log").textContent = `backend unreachable: ${err}`;
  }
}

async function run(command, args) {
  try {
    const result = await tauriCore.invoke(command, args || {});
    if (typeof result === "string") {
      document.getElementById("log").textContent = result;
    }
  } catch (err) {
    document.getElementById("log").textContent = `${command} failed: ${err}`;
  }
  refresh();
}

document.getElementById("btn-embedded").addEventListener("click", () => run("connect_embedded"));
document.getElementById("btn-central").addEventListener("click", () => run("connect_central"));
document.getElementById("btn-disconnect").addEventListener("click", () => run("disconnect"));
document.getElementById("btn-rescan").addEventListener("click", () => run("rescan"));
document.querySelectorAll("[data-fire]").forEach((button) => {
  button.addEventListener("click", () => run("test_fire", { kind: button.dataset.fire }));
});

const logLine = document.getElementById("log");

if (!tauriCore) {
  logLine.textContent =
    "tauri bridge missing: rebuild with app.withGlobalTauri enabled";
} else {
  setInterval(refresh, 1000);
  refresh();
}
