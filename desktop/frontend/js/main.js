import { getStatus, init, startPolling } from "./backend.js";
import { els } from "./dom.js";
import { bindHome, flashOnFire, renderHome, renderRail } from "./home.js";
import { bindLogs, renderLogs } from "./logs.js";
import { bindSettings, renderConfig, renderUpdates } from "./settings.js";
import { renderTriggers } from "./triggers.js";
import { bindToys, renderToys } from "./toys.js";

const REFRESH_MS = 1000;

function renderAll() {
  if (!getStatus()) return;
  renderRail();
  renderHome();
  renderTriggers();
  renderToys();
  renderConfig();
  renderUpdates();
  renderLogs();
  flashOnFire();
}

const VIEWS = [...document.querySelectorAll(".view")].map((panel) => panel.dataset.panel);

function showView(view) {
  if (!VIEWS.includes(view)) {
    view = "home";
  }
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
  showView(raw === "config" ? "settings" : raw);
}

if (!window.__TAURI__) {
  els.railDemo.hidden = false;
  els.railLine1.textContent = "preview data";
}

init(renderAll);
bindHome();
bindToys();
bindSettings();
bindLogs();
window.addEventListener("hashchange", routeFromHash);
routeFromHash();
startPolling(REFRESH_MS);
