import { getStatus, run } from "./backend.js";
import { els, h } from "./dom.js";

let chosenBackend = "embedded";

export function toyLine() {
  const status = getStatus();
  switch (status.toy_mode) {
    case "Embedded": return `Embedded, ${status.devices.length} device${status.devices.length === 1 ? "" : "s"}`;
    case "ExternalCentral": return `Intiface Central, ${status.devices.length} device${status.devices.length === 1 ? "" : "s"}`;
    default: return "Not connected";
  }
}

export function renderToys() {
  const status = getStatus();
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

export function bindToys() {
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
  els.btnToysTest.addEventListener("click", () => run("test_toys", {}, [els.btnToysTest]));
}
