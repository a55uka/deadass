import { getStatus, run } from "./backend.js";
import { els } from "./dom.js";
import { renderLogInto } from "./logs.js";
import { toyLine } from "./toys.js";

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

export function renderRail() {
  const { phase, text } = trailInfo();
  els.dot.className = `status-dot is-${phase}`;
  els.dot.title = text;
  els.railLine1.textContent = text;
  els.railLine2.textContent = toyLine();
}

let lastFiredLine = null;

export function flashOnFire() {
  const status = getStatus();
  const last = status.log_lines.at(-1) ?? "";
  if (!/^vibrate /.test(last) || last === lastFiredLine) return;
  lastFiredLine = last;
  els.dot.classList.add("is-firing");
  setTimeout(() => els.dot.classList.remove("is-firing"), 450);
}

export function renderHome() {
  const status = getStatus();
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

export function bindHome() {
  for (const btn of els.sourceSeg.children) {
    btn.addEventListener("click", () => run("set_data_source", { source: btn.dataset.source }, [btn]));
  }
}
