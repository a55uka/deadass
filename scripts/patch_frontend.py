path = 'desktop/frontend/app.js'
src = open(path, encoding='utf-8').read()

# trigger row: shock controls
old = """    const pattern = document.createElement("select");
    pattern.title = "pattern";
    for (const option of ["vibrate", "pulse", "ramp"]) {
      pattern.append(Object.assign(document.createElement("option"), { value: option, textContent: option }));
    }
    pattern.value = trigger.pattern;
    pattern.addEventListener("change", () => pushTrigger(trigger.kind, row));

    label.append(enabled, strength, duration, pattern);
    row.append(label);
    elements.triggers.append(row);
  }
}"""
new = """    const pattern = document.createElement("select");
    pattern.title = "pattern";
    for (const option of ["vibrate", "pulse", "ramp"]) {
      pattern.append(Object.assign(document.createElement("option"), { value: option, textContent: option }));
    }
    pattern.value = trigger.pattern;
    pattern.addEventListener("change", () => pushTrigger(trigger.kind, row));

    const shockEnabled = document.createElement("input");
    shockEnabled.type = "checkbox";
    shockEnabled.checked = Boolean(trigger.shock_enabled);
    shockEnabled.title = "shock";
    shockEnabled.addEventListener("change", () => pushTrigger(trigger.kind, row));

    const shockIntensity = document.createElement("input");
    shockIntensity.type = "number";
    shockIntensity.min = "1";
    shockIntensity.max = "100";
    shockIntensity.value = String(trigger.shock_intensity ?? 20);
    shockIntensity.title = "shock intensity (1-100)";
    shockIntensity.addEventListener("change", () => pushTrigger(trigger.kind, row));

    const shockDuration = document.createElement("input");
    shockDuration.type = "number";
    shockDuration.min = "300";
    shockDuration.max = "30000";
    shockDuration.step = "100";
    shockDuration.value = String(trigger.shock_duration_ms ?? 300);
    shockDuration.title = "shock duration (ms)";
    shockDuration.addEventListener("change", () => pushTrigger(trigger.kind, row));

    label.append(enabled, strength, duration, pattern, shockEnabled, shockIntensity, shockDuration);
    row.append(label);
    elements.triggers.append(row);
  }
}"""
assert old in src, "trigger row not found"
src = src.replace(old, new)

# payload gains shock fields
old = """function triggerFromRow(kind, row) {
  const [enabled, strength, duration, pattern] = row.querySelectorAll("input, select");
  return {
    kind,
    enabled: enabled.checked,
    strength: Number(strength.value),
    duration_ms: Number(duration.value),
    retrigger_cooldown_ms: 500,
    pattern: pattern.value,
  };
}"""
new = """function triggerFromRow(kind, row) {
  const [enabled, strength, duration, pattern, shockEnabled, shockIntensity, shockDuration] =
    row.querySelectorAll("input, select");
  return {
    kind,
    enabled: enabled.checked,
    strength: Number(strength.value),
    duration_ms: Number(duration.value),
    retrigger_cooldown_ms: 500,
    pattern: pattern.value,
    shock_enabled: shockEnabled.checked,
    shock_intensity: Number(shockIntensity.value),
    shock_duration_ms: Number(shockDuration.value),
  };
}"""
assert old in src, "triggerFromRow not found"
src = src.replace(old, new)

open(path, 'w', encoding='utf-8').write(src)

# index.html: OpenShock connection section
path = 'desktop/frontend/index.html'
src = open(path, encoding='utf-8').read()
old = """      <section>
        <h2>Triggers</h2>
        <div id="triggers"></div>
      </section>"""
new = """      <section>
        <h2>Triggers</h2>
        <p class="muted">
          Each row: enable, vibration strength, duration, pattern, then shock
          on/off, intensity (1-100) and duration (ms).
        </p>
        <div id="triggers"></div>
      </section>

      <section>
        <h2>OpenShock</h2>
        <div class="row">
          <label><input type="checkbox" id="openshock-enabled" /> Enabled</label>
          <label>Base URL <input type="text" id="openshock-base-url" placeholder="https://api.openshock.app" /></label>
        </div>
        <div class="row">
          <label>API token <input type="password" id="openshock-token" /></label>
          <label>Shocker ID <input type="text" id="openshock-shocker-id" /></label>
          <button id="btn-openshock-save">Save</button>
          <button id="btn-openshock-test">Test</button>
        </div>
        <p id="openshock-status" class="muted" hidden></p>
      </section>"""
assert old in src, "index.html triggers section not found"
src = src.replace(old, new)
open(path, 'w', encoding='utf-8').write(src)
print("frontend patched")
