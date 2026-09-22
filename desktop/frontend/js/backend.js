import { disable, els, h, pending } from "./dom.js";

const tauri = window.__TAURI__ && window.__TAURI__.core;
const invoke = window.__demoBridge ?? ((command, args) => tauri.invoke(command, args));

let status = null;
let afterUpdate = () => {};

export function getStatus() {
  return status;
}

export function init(onUpdate) {
  afterUpdate = onUpdate;
}

export async function startPolling(intervalMs) {
  setInterval(poll, intervalMs);
  await poll();
}

export async function run(command, args = {}, controls = []) {
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
    afterUpdate();
  }
}

export async function poll() {
  try {
    status = await invoke("get_status", {});
    afterUpdate();
  } catch (error) {
    showCommandError(`Backend unreachable: ${error}`);
    els.dot.className = "status-dot is-off";
  }
}

export function showCommandError(message) {
  els.commandError.hidden = !message;
  els.commandError.replaceChildren();
  if (message) {
    els.commandError.append(h("span", {}, message), h("button", { type: "button", onclick: () => showCommandError(null) }, "Dismiss"));
  }
}
