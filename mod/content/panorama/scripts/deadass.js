(function () {
  "use strict";

  var LOG_PREFIX = "[DEADASS]";
  var MOD_VERSION = "0.1.0";
  var POLL_INTERVAL_SECONDS = 0.1;
  var context = $.GetContextPanel();
  var localPlayerPanel = null;
  var deathBaselineEstablished = false;
  var wasDead = false;
  var sequence = 0;
  var abilityRoot = null;
  var abilityHeroIdentity = null;
  var abilityPanels = [];
  var abilityStates = [];
  var sessionId = Date.now().toString(36) + "-" + Math.floor(Math.random() * 0x1000000).toString(36);
  var BASELINE_SETTLE_POLLS = 250;
  var baselineSettlePollsRemaining = 0;
  var lastKillStreakCount = null;
  var killStreakNullPolls = 0;
  var KILL_STREAK_RESET_POLLS = 3;
  var creditedAssistPanels = [];
  var damageImpactPollCounter = 0;
  var DAMAGE_IMPACT_POLL_INTERVAL_POLLS = 3;

  function emit(eventName, fields) {
    var payload = {
      schema: 1,
      event: eventName,
      mod_version: MOD_VERSION,
      session_id: sessionId,
      client_time_ms: Date.now()
    };
    if (fields) {
      for (var key in fields) {
        if (Object.prototype.hasOwnProperty.call(fields, key)) {
          payload[key] = fields[key];
        }
      }
    }
    $.Msg(LOG_PREFIX + JSON.stringify(payload));
  }

  function emitAction(eventName, fields) {
    sequence++;
    fields.sequence = sequence;
    emit(eventName, fields);
  }

  function isValidPanel(panel) {
    try {
      return !!(panel && panel.IsValid && panel.IsValid());
    } catch (_error) {
      return false;
    }
  }

  function panelHasClass(panel, className) {
    try {
      return !!(isValidPanel(panel) && panel.BHasClass && panel.BHasClass(className));
    } catch (_error) {
      return false;
    }
  }

  function panelProperty(panel, propertyName) {
    try {
      var value = panel[propertyName];
      if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
        return value;
      }
    } catch (_error) {
    }
    return null;
  }

  function panelAttribute(panel, attributeName) {
    if (!isValidPanel(panel) || !panel.GetAttributeString) {
      return null;
    }
    var missing = "__deadass_missing__";
    try {
      var value = panel.GetAttributeString(attributeName, missing);
      return value !== missing && value !== "" ? value : null;
    } catch (_error) {
      return null;
    }
  }

  function panelChildren(panel) {
    try {
      if (panel.Children) {
        return panel.Children();
      }
      if (panel.GetChildCount && panel.GetChild) {
        var children = [];
        for (var i = 0; i < panel.GetChildCount(); i++) {
          children.push(panel.GetChild(i));
        }
        return children;
      }
    } catch (_error) {
    }
    return [];
  }

  function findChildById(panel, id) {
    if (!isValidPanel(panel)) {
      return null;
    }
    try {
      if (panel.id === id) {
        return panel;
      }
      if (panel.FindChildTraverse) {
        return panel.FindChildTraverse(id);
      }
    } catch (_error) {
    }
    return null;
  }

  function findChildrenWithClass(panel, className) {
    if (!isValidPanel(panel)) {
      return [];
    }
    try {
      if (panel.FindChildrenWithClassTraverse) {
        return panel.FindChildrenWithClassTraverse(className);
      }
    } catch (_error) {
    }
    return [];
  }

  function firstPanelWithType(panel, panelType) {
    if (!isValidPanel(panel)) {
      return null;
    }
    if (panelProperty(panel, "paneltype") === panelType) {
      return panel;
    }
    var children = panelChildren(panel);
    for (var i = 0; i < children.length; i++) {
      var match = firstPanelWithType(children[i], panelType);
      if (match) {
        return match;
      }
    }
    return null;
  }

  function textFromClass(panel, className) {
    var matches = findChildrenWithClass(panel, className);
    return matches.length > 0 ? panelProperty(matches[0], "text") : null;
  }

  function highestContextAncestor() {
    var panel = context;
    for (var i = 0; i < 64 && isValidPanel(panel); i++) {
      var parent = null;
      try {
        parent = panel.GetParent ? panel.GetParent() : null;
      } catch (_error) {
        parent = null;
      }
      if (!isValidPanel(parent)) {
        break;
      }
      panel = parent;
    }
    return panel;
  }

  function findAbilityRoot() {
    return findChildById(highestContextAncestor(), "hud_signature");
  }

  function abilityEntries(root) {
    return findChildrenWithClass(root, "ability_container");
  }

  function stableIdentity(panel, attributeNames) {
    for (var i = 0; i < attributeNames.length; i++) {
      var value = panelAttribute(panel, attributeNames[i]);
      if (value !== null) {
        return attributeNames[i] + ":" + value;
      }
    }
    return null;
  }

  function heroIdentity(root) {
    return stableIdentity(root, ["hero_id", "hero_name", "heroname", "unit_name", "entity_index"]);
  }

  function abilityIdentity(panel) {
    return stableIdentity(panel, ["ability_id", "ability", "ability_name", "entity_index"]);
  }

  function integerPanelProperty(panel, propertyName) {
    var rawValue = panelProperty(panel, propertyName);
    if (rawValue === null) {
      return null;
    }
    var value = Number(rawValue);
    if (!isFinite(value) || Math.floor(value) !== value || value < 0) {
      return null;
    }
    return value;
  }

  function abilitySnapshot(panel, slotIndex) {
    if (!isValidPanel(panel)) {
      return null;
    }
    var charged = panelHasClass(panel, "has_stack_charges");
    var charges = null;
    var maxCharges = null;
    if (charged) {
      var chargeContainers = findChildrenWithClass(panel, "stack_charges");
      if (chargeContainers.length === 0) {
        return null;
      }
      var progress = null;
      for (var chargeIndex = 0; chargeIndex < chargeContainers.length && !progress; chargeIndex++) {
        progress = firstPanelWithType(chargeContainers[chargeIndex], "ProgressBarWithMiddle");
      }
      if (!progress) {
        return null;
      }
      charges = integerPanelProperty(progress, "lowervalue");
      maxCharges = integerPanelProperty(progress, "max");
      if (charges === null || maxCharges === null || charges > maxCharges) {
        return null;
      }
    }
    return {
      slot: slotIndex,
      identity: abilityIdentity(panel),
      name: textFromClass(panel, "ability_name"),
      charged: charged,
      charges: charges,
      max_charges: maxCharges,
      cooling_down: panelHasClass(panel, "cooling_down"),
      active: panelHasClass(panel, "active")
    };
  }

  function resetAbilityState() {
    abilityRoot = null;
    abilityHeroIdentity = null;
    abilityPanels = [];
    abilityStates = [];
  }

  function baselineAbilityPanels(root, panels) {
    abilityRoot = root;
    abilityHeroIdentity = heroIdentity(root);
    abilityPanels = panels.slice(0);
    abilityStates = [];
    for (var i = 0; i < panels.length; i++) {
      abilityStates.push(abilitySnapshot(panels[i], i));
    }
  }

  function samePanelSet(panels) {
    if (panels.length !== abilityPanels.length) {
      return false;
    }
    for (var i = 0; i < panels.length; i++) {
      if (panels[i] !== abilityPanels[i] || !isValidPanel(panels[i])) {
        return false;
      }
    }
    return true;
  }

  function eventFields(snapshot, detection) {
    var fields = {
      ability_slot: snapshot.slot,
      detection: detection
    };
    if (typeof snapshot.name === "string" && snapshot.name !== "") {
      fields.ability_name = snapshot.name;
    }
    return fields;
  }

  function detectAbilityTransitions(previous, current) {
    var chargeDecreased = current.charged && current.charges < previous.charges;
    var chargeIncreased = current.charged && current.charges > previous.charges;
    var cooldownStarted = !previous.cooling_down && current.cooling_down;
    var activated = !previous.active && current.active;
    var cooldownFinished = previous.cooling_down && !current.cooling_down;

    if (current.charged) {
      if (chargeDecreased) {
        emitAction("ability_used", eventFields(current, "charge_decrement"));
      }
    } else if (cooldownStarted || activated) {
      var useDetection = cooldownStarted && activated
        ? "cooldown_started_and_activated"
        : (cooldownStarted ? "cooldown_started" : "activated");
      emitAction("ability_used", eventFields(current, useDetection));
    }

    if (cooldownFinished || chargeIncreased) {
      var readyDetection = cooldownFinished && chargeIncreased
        ? "cooldown_finished_and_charge_restored"
        : (cooldownFinished ? "cooldown_finished" : "charge_restored");
      emitAction("ability_ready", eventFields(current, readyDetection));
    }
  }

  function pollAbilities(forceBaseline) {
    var root = findAbilityRoot();
    if (!isValidPanel(root)) {
      resetAbilityState();
      return;
    }
    var panels = abilityEntries(root);
    var currentHeroIdentity = heroIdentity(root);
    if (forceBaseline || abilityRoot !== root || !samePanelSet(panels) || currentHeroIdentity !== abilityHeroIdentity) {
      baselineAbilityPanels(root, panels);
      return;
    }
    for (var slotIndex = 0; slotIndex < panels.length; slotIndex++) {
      var current = abilitySnapshot(panels[slotIndex], slotIndex);
      var previous = abilityStates[slotIndex];
      if (!current || !previous) {
        abilityStates[slotIndex] = current;
        continue;
      }
      if (current.identity !== previous.identity
        || current.charged !== previous.charged
        || current.max_charges !== previous.max_charges) {
        abilityStates[slotIndex] = current;
        continue;
      }
      detectAbilityTransitions(previous, current);
      abilityStates[slotIndex] = current;
    }
  }

  function killStreakCount(player) {
    var textNode = findChildById(player, "KillStreakText");
    if (!isValidPanel(textNode)) {
      return null;
    }
    var children = panelChildren(textNode);
    for (var i = 0; i < children.length; i++) {
      if (panelProperty(children[i], "paneltype") !== "Label") {
        continue;
      }
      var text = panelProperty(children[i], "text");
      if (text === null) {
        return null;
      }
      var value = Number(text);
      return isFinite(value) && value >= 0 ? Math.floor(value) : null;
    }
    return null;
  }

  function pollOwnKillStreak(player, forceBaseline) {
    var count = killStreakCount(player);
    if (count === null) {
      killStreakNullPolls++;
      if (killStreakNullPolls >= KILL_STREAK_RESET_POLLS) {
        lastKillStreakCount = null;
      }
      return;
    }
    killStreakNullPolls = 0;
    if (forceBaseline) {
      lastKillStreakCount = count;
      return;
    }
    if (lastKillStreakCount === null) {
      if (count >= 1) {
        emitAction("kill", { detection: "kill_streak_counter_increment" });
      }
    } else if (count > lastKillStreakCount) {
      emitAction("kill", { detection: "kill_streak_counter_increment" });
    }
    lastKillStreakCount = count;
  }

  function pollDamageImpactAssists(root) {
    damageImpactPollCounter++;
    if (damageImpactPollCounter < DAMAGE_IMPACT_POLL_INTERVAL_POLLS) {
      return;
    }
    damageImpactPollCounter = 0;
    var container = findChildById(root, "damageImpactInfo");
    if (!isValidPanel(container)) {
      return;
    }
    var stillValid = [];
    for (var p = 0; p < creditedAssistPanels.length; p++) {
      if (isValidPanel(creditedAssistPanels[p])) {
        stillValid.push(creditedAssistPanels[p]);
      }
    }
    creditedAssistPanels = stillValid;
    var children = panelChildren(container);
    for (var i = 0; i < children.length; i++) {
      var child = children[i];
      if (!isValidPanel(child) || !panelHasClass(child, "assist")) {
        continue;
      }
      if (creditedAssistPanels.indexOf(child) !== -1) {
        continue;
      }
      creditedAssistPanels.push(child);
      emitAction("assist", { detection: "damage_impact_assist_class" });
    }
  }

  function findLocalPlayerPanel() {
    if (isValidPanel(localPlayerPanel)) {
      return localPlayerPanel;
    }
    localPlayerPanel = null;
    deathBaselineEstablished = false;
    lastKillStreakCount = null;
    var panels = context.FindChildrenWithClassTraverse("LocalPlayer");
    for (var i = 0; i < panels.length; i++) {
      if (panels[i].paneltype === "CitadelHudTopBarPlayer") {
        localPlayerPanel = panels[i];
        return localPlayerPanel;
      }
    }
    return null;
  }

  function pollState() {
    if (!isValidPanel(context)) {
      return;
    }
    var player = findLocalPlayerPanel();
    if (!player) {
      resetAbilityState();
      $.Schedule(POLL_INTERVAL_SECONDS, pollState);
      return;
    }
    var isDead = panelHasClass(player, "Dead");
    var forceAbilityBaseline = false;
    if (!deathBaselineEstablished) {
      wasDead = isDead;
      deathBaselineEstablished = true;
      baselineSettlePollsRemaining = BASELINE_SETTLE_POLLS;
      forceAbilityBaseline = true;
    } else if (baselineSettlePollsRemaining > 0) {
      baselineSettlePollsRemaining--;
      wasDead = isDead;
    } else if (isDead !== wasDead) {
      forceAbilityBaseline = true;
      if (isDead) {
        emitAction("death", { detection: "top_bar_local_player_dead_class" });
      } else {
        emitAction("respawn", { detection: "top_bar_local_player_dead_class" });
      }
      wasDead = isDead;
    }
    pollAbilities(forceAbilityBaseline || isDead);
    pollOwnKillStreak(player, forceAbilityBaseline || baselineSettlePollsRemaining > 0);
    pollDamageImpactAssists(highestContextAncestor());
    $.Schedule(POLL_INTERVAL_SECONDS, pollState);
  }

  emit("hook_ready", { poll_interval_ms: POLL_INTERVAL_SECONDS * 1000 });
  pollState();
})();
