import { getStatus, showCommandError } from "./backend.js";
import { els, h } from "./dom.js";

export function logLineClass(line) {
  if (/^vibrate /.test(line) || /^connected/.test(line)) return "is-fire";
  if (/failed|error|could not/i.test(line)) return "is-error";
  if (/^suppress |waiting|missing/i.test(line)) return "is-dim";
  return "";
}

export function renderLogInto(el, lines) {
  el.replaceChildren(...lines.map((line) => h("span", { class: `log-line ${logLineClass(line)}` }, line + "\n")));
}

let logSignature = null;
let logStick = true;

export function renderLogs() {
  const lines = getStatus().log_lines;
  const signature = `${lines.length}|${lines.at(-1) ?? ""}`;
  if (signature !== logSignature) {
    logSignature = signature;
    renderLogInto(els.log, lines);
    if (logStick) els.log.scrollTop = els.log.scrollHeight;
    els.btnLogBottom.hidden = logStick;
  }
}

export function bindLogs() {
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
    const status = getStatus();
    if (!status) return;
    try {
      await navigator.clipboard.writeText(status.log_lines.join("\n"));
      els.btnLogCopy.textContent = "Copied";
      setTimeout(() => { els.btnLogCopy.textContent = "Copy"; }, 1200);
    } catch {
      showCommandError("Copy failed: the clipboard refused access");
    }
  });
}
