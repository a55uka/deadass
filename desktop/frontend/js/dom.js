export const els = {
  nav: document.getElementById("nav"),
  dot: document.getElementById("rail-dot"),
  railLine1: document.getElementById("rail-line1"),
  railLine2: document.getElementById("rail-line2"),
  railDemo: document.getElementById("rail-demo"),
  stage: document.getElementById("stage"),
  commandError: document.getElementById("command-error"),
  sourceSeg: document.getElementById("source-seg"),
  sourcePill: document.getElementById("home-source-pill"),
  sourceDetail: document.getElementById("home-source-detail"),
  homeToys: document.getElementById("home-toys"),
  homeToyError: document.getElementById("home-toy-error"),
  homeLog: document.getElementById("home-log"),
  homeUpdate: document.getElementById("home-update"),
  homeUpdateText: document.getElementById("home-update-text"),
  triggerGroups: document.getElementById("trigger-groups"),
  toyBackendSeg: document.getElementById("toy-backend-seg"),
  btnConnect: document.getElementById("btn-connect"),
  btnDisconnect: document.getElementById("btn-disconnect"),
  btnRescan: document.getElementById("btn-rescan"),
  devices: document.getElementById("devices"),
  devicesEmpty: document.getElementById("devices-empty"),
  toyError: document.getElementById("toy-error"),
  toysCount: document.getElementById("toys-count"),
  cfgGain: document.getElementById("cfg-master-gain"),
  cfgGainOut: document.getElementById("cfg-master-gain-out"),
  cfgCap: document.getElementById("cfg-cap"),
  cfgCapOut: document.getElementById("cfg-cap-out"),
  cfgMute: document.getElementById("cfg-mute"),
  cfgUrl: document.getElementById("cfg-central-url"),
  cfgDllPath: document.getElementById("cfg-dll-path"),
  cfgModPort: document.getElementById("cfg-mod-port"),
  cfgDllPort: document.getElementById("cfg-dll-port"),
  cfgDebug: document.getElementById("cfg-debug"),
  cfgOsEnabled: document.getElementById("cfg-openshock-enabled"),
  cfgOsToken: document.getElementById("cfg-openshock-token"),
  cfgOsBaseUrl: document.getElementById("cfg-openshock-base-url"),
  cfgOsState: document.getElementById("cfg-openshock-state"),
  cfgOsHint: document.getElementById("cfg-openshock-hint"),
  osRoster: document.getElementById("openshock-roster"),
  btnOsAdd: document.getElementById("btn-openshock-add"),
  btnOsSave: document.getElementById("btn-openshock-save"),
  btnOsTest: document.getElementById("btn-openshock-test"),
  btnToysTest: document.getElementById("btn-toys-test"),
  configPath: document.getElementById("config-path"),
  log: document.getElementById("log"),
  btnLogCopy: document.getElementById("btn-log-copy"),
  btnLogBottom: document.getElementById("btn-log-bottom"),
  updCurrent: document.getElementById("upd-current"),
  updLatest: document.getElementById("upd-latest"),
  updStatus: document.getElementById("upd-status"),
  btnUpdCheck: document.getElementById("btn-upd-check"),
  btnUpdOffsets: document.getElementById("btn-upd-offsets"),
  btnUpdApp: document.getElementById("btn-upd-app"),
};

export const pending = new Set();

export function h(tag, attrs = {}, ...children) {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value == null || value === false) continue;
    if (key === "class") el.className = value;
    else if (key === "dataset") Object.assign(el.dataset, value);
    else if (key.startsWith("on")) el.addEventListener(key.slice(2), value);
    else el.setAttribute(key, value === true ? "" : value);
  }
  for (const child of children.flat()) {
    if (child == null) continue;
    el.append(child.nodeType ? child : document.createTextNode(child));
  }
  return el;
}

export function disable(control, value) {
  if ("disabled" in control) control.disabled = value;
}

export function setValue(control, apply) {
  if (pending.has(control) || control === document.activeElement) return;
  apply();
}
