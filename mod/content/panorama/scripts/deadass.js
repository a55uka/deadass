(function () {
  "use strict";

  var CONFIG = {
    logPrefix: "[DEADASS]",
    modVersion: "0.1.0",
    pollIntervalSeconds: 0.1,
    baselineSettlePolls: 250,
    killStreakResetPolls: 3,
    damageImpactIntervalPolls: 3,
    killBannerIntervalPolls: 3,
    killBannerCooldownMs: 1500,
    respawnDeathCooldownMs: 2000,
    killBannerPatterns: [
      /first blood/i,
      /kill streak/i,
      /\bdouble kill\b/i,
      /\btriple kill\b/i,
      /multi.?kill/i,
      /killing spree/i,
      /rampage/i,
      /unstoppable/i
    ]
  };

  var Bridge = {
    sessionId: Date.now().toString(36) + "-" + Math.floor(Math.random() * 0x1000000).toString(36),
    sequence: 0,
    lastKillMs: 0,
    lastDeathMs: 0,

    send: function (eventName, fields) {
      var payload = {
        schema: 1,
        event: eventName,
        mod_version: CONFIG.modVersion,
        session_id: Bridge.sessionId,
        client_time_ms: Date.now()
      };
      for (var key in fields) {
        if (Object.prototype.hasOwnProperty.call(fields, key)) {
          payload[key] = fields[key];
        }
      }
      $.Msg(CONFIG.logPrefix + JSON.stringify(payload));
    },

    action: function (eventName, fields) {
      Bridge.sequence++;
      fields.sequence = Bridge.sequence;
      Bridge.send(eventName, fields);
    },

    kill: function (fields) {
      Bridge.lastKillMs = Date.now();
      Bridge.action("kill", fields);
    },

    death: function (fields) {
      Bridge.lastDeathMs = Date.now();
      Bridge.action("death", fields);
    }
  };

  var Panels = {
    valid: function (panel) {
      try {
        return !!(panel && panel.IsValid && panel.IsValid());
      } catch (_error) {
        return false;
      }
    },

    hasClass: function (panel, className) {
      try {
        return !!(Panels.valid(panel) && panel.BHasClass && panel.BHasClass(className));
      } catch (_error) {
        return false;
      }
    },

    property: function (panel, name) {
      try {
        var value = panel[name];
        if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
          return value;
        }
      } catch (_error) {
      }
      return null;
    },

    attribute: function (panel, name) {
      if (!Panels.valid(panel) || !panel.GetAttributeString) {
        return null;
      }
      var missing = "__deadass_missing__";
      try {
        var value = panel.GetAttributeString(name, missing);
        return value !== missing && value !== "" ? value : null;
      } catch (_error) {
        return null;
      }
    },

    children: function (panel) {
      try {
        if (panel.Children) {
          return panel.Children();
        }
        if (panel.GetChildCount && panel.GetChild) {
          var out = [];
          for (var i = 0; i < panel.GetChildCount(); i++) {
            out.push(panel.GetChild(i));
          }
          return out;
        }
      } catch (_error) {
      }
      return [];
    },

    childById: function (panel, id) {
      if (!Panels.valid(panel)) {
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
    },

    childrenWithClass: function (panel, className) {
      if (!Panels.valid(panel)) {
        return [];
      }
      try {
        if (panel.FindChildrenWithClassTraverse) {
          return panel.FindChildrenWithClassTraverse(className);
        }
      } catch (_error) {
      }
      return [];
    },

    firstWithType: function (panel, type) {
      if (!Panels.valid(panel)) {
        return null;
      }
      if (Panels.property(panel, "paneltype") === type) {
        return panel;
      }
      var children = Panels.children(panel);
      for (var i = 0; i < children.length; i++) {
        var match = Panels.firstWithType(children[i], type);
        if (match) {
          return match;
        }
      }
      return null;
    },

    textInClass: function (panel, className) {
      var matches = Panels.childrenWithClass(panel, className);
      return matches.length > 0 ? Panels.property(matches[0], "text") : null;
    },

    integerProperty: function (panel, name) {
      var raw = Panels.property(panel, name);
      if (raw === null) {
        return null;
      }
      var value = Number(raw);
      if (!isFinite(value) || Math.floor(value) !== value || value < 0) {
        return null;
      }
      return value;
    },

    root: function (context) {
      var panel = context;
      for (var i = 0; i < 64 && Panels.valid(panel); i++) {
        var parent = null;
        try {
          parent = panel.GetParent ? panel.GetParent() : null;
        } catch (_error) {
          parent = null;
        }
        if (!Panels.valid(parent)) {
          break;
        }
        panel = parent;
      }
      return panel;
    },

    labelTexts: function (panel, out) {
      if (!Panels.valid(panel)) {
        return;
      }
      if (Panels.property(panel, "paneltype") === "Label" && Panels.property(panel, "visible") !== false) {
        var text = Panels.property(panel, "text");
        if (typeof text === "string" && text !== "" && text.charAt(0) !== "{") {
          out.push(text);
        }
      }
      var children = Panels.children(panel);
      for (var i = 0; i < children.length; i++) {
        Panels.labelTexts(children[i], out);
      }
    },

    stableIdentity: function (panel, names) {
      for (var i = 0; i < names.length; i++) {
        var value = Panels.attribute(panel, names[i]);
        if (value !== null) {
          return names[i] + ":" + value;
        }
      }
      return null;
    }
  };

  var Abilities = {
    root: null,
    heroIdentity: null,
    panels: [],
    states: [],

    signatureRoot: function (context) {
      return Panels.childById(Panels.root(context), "hud_signature");
    },

    entries: function (root) {
      return Panels.childrenWithClass(root, "ability_container");
    },

    snapshot: function (panel, slot) {
      if (!Panels.valid(panel)) {
        return null;
      }
      var charged = Panels.hasClass(panel, "has_stack_charges");
      var charges = null;
      var maxCharges = null;
      if (charged) {
        var containers = Panels.childrenWithClass(panel, "stack_charges");
        if (containers.length === 0) {
          return null;
        }
        var progress = null;
        for (var i = 0; i < containers.length && !progress; i++) {
          progress = Panels.firstWithType(containers[i], "ProgressBarWithMiddle");
        }
        if (!progress) {
          return null;
        }
        charges = Panels.integerProperty(progress, "lowervalue");
        maxCharges = Panels.integerProperty(progress, "max");
        if (charges === null || maxCharges === null || charges > maxCharges) {
          return null;
        }
      }
      return {
        slot: slot,
        identity: Panels.stableIdentity(panel, ["ability_id", "ability", "ability_name", "entity_index"]),
        name: Panels.textInClass(panel, "ability_name"),
        charged: charged,
        charges: charges,
        max_charges: maxCharges,
        cooling_down: Panels.hasClass(panel, "cooling_down"),
        active: Panels.hasClass(panel, "active")
      };
    },

    reset: function () {
      Abilities.root = null;
      Abilities.heroIdentity = null;
      Abilities.panels = [];
      Abilities.states = [];
    },

    baseline: function (root, panels) {
      Abilities.root = root;
      Abilities.heroIdentity = Panels.stableIdentity(root, ["hero_id", "hero_name", "heroname", "unit_name", "entity_index"]);
      Abilities.panels = panels.slice(0);
      Abilities.states = [];
      for (var i = 0; i < panels.length; i++) {
        Abilities.states.push(Abilities.snapshot(panels[i], i));
      }
    },

    samePanels: function (panels) {
      if (panels.length !== Abilities.panels.length) {
        return false;
      }
      for (var i = 0; i < panels.length; i++) {
        if (panels[i] !== Abilities.panels[i] || !Panels.valid(panels[i])) {
          return false;
        }
      }
      return true;
    },

    fields: function (snapshot, detection) {
      var fields = { ability_slot: snapshot.slot, detection: detection };
      if (typeof snapshot.name === "string" && snapshot.name !== "") {
        fields.ability_name = snapshot.name;
      }
      return fields;
    },

    transitions: function (previous, current) {
      var chargeUsed = current.charged && current.charges < previous.charges;
      var chargeRestored = current.charged && current.charges > previous.charges;
      var cooldownStarted = !previous.cooling_down && current.cooling_down;
      var activated = !previous.active && current.active;
      var cooldownFinished = previous.cooling_down && !current.cooling_down;

      if (current.charged) {
        if (chargeUsed) {
          Bridge.action("ability_used", Abilities.fields(current, "charge_decrement"));
        }
      } else if (cooldownStarted || activated) {
        var useDetection = cooldownStarted && activated
          ? "cooldown_started_and_activated"
          : (cooldownStarted ? "cooldown_started" : "activated");
        Bridge.action("ability_used", Abilities.fields(current, useDetection));
      }

      if (cooldownFinished || chargeRestored) {
        var readyDetection = cooldownFinished && chargeRestored
          ? "cooldown_finished_and_charge_restored"
          : (cooldownFinished ? "cooldown_finished" : "charge_restored");
        Bridge.action("ability_ready", Abilities.fields(current, readyDetection));
      }
    },

    poll: function (context, forceBaseline, suspended) {
      var root = Abilities.signatureRoot(context);
      if (!Panels.valid(root)) {
        Abilities.reset();
        return;
      }
      var panels = Abilities.entries(root);
      var hero = Panels.stableIdentity(root, ["hero_id", "hero_name", "heroname", "unit_name", "entity_index"]);
      if (forceBaseline || Abilities.root !== root || !Abilities.samePanels(panels) || hero !== Abilities.heroIdentity) {
        Abilities.baseline(root, panels);
        return;
      }
      if (suspended) {
        return;
      }
      for (var slot = 0; slot < panels.length; slot++) {
        var current = Abilities.snapshot(panels[slot], slot);
        var previous = Abilities.states[slot];
        if (!current || !previous) {
          Abilities.states[slot] = current;
          continue;
        }
        if (current.identity !== previous.identity || current.charged !== previous.charged || current.max_charges !== previous.max_charges) {
          Abilities.states[slot] = current;
          continue;
        }
        Abilities.transitions(previous, current);
        Abilities.states[slot] = current;
      }
    }
  };

  var KillStreak = {
    lastCount: null,
    nullPolls: 0,

    count: function (player) {
      var textNode = Panels.childById(player, "KillStreakText");
      if (!Panels.valid(textNode)) {
        return null;
      }
      var children = Panels.children(textNode);
      for (var i = 0; i < children.length; i++) {
        if (Panels.property(children[i], "paneltype") !== "Label") {
          continue;
        }
        var text = Panels.property(children[i], "text");
        if (text === null) {
          return null;
        }
        var value = Number(text);
        return isFinite(value) && value >= 0 ? Math.floor(value) : null;
      }
      return null;
    },

    poll: function (player, forceBaseline, settling) {
      var count = KillStreak.count(player);
      if (count === null) {
        KillStreak.nullPolls++;
        if (KillStreak.nullPolls >= CONFIG.killStreakResetPolls) {
          KillStreak.lastCount = null;
        }
        return;
      }
      KillStreak.nullPolls = 0;
      if (forceBaseline || settling) {
        KillStreak.lastCount = count;
        return;
      }
      if (KillStreak.lastCount === null) {
        if (count >= 1) {
          Bridge.kill({ detection: "kill_streak_counter_increment" });
        }
      } else if (count > KillStreak.lastCount) {
        Bridge.kill({ detection: "kill_streak_counter_increment" });
      }
      KillStreak.lastCount = count;
    },

    reset: function () {
      KillStreak.lastCount = null;
      KillStreak.nullPolls = 0;
    }
  };

  var Assists = {
    credited: [],
    polls: 0,

    poll: function (root) {
      Assists.polls++;
      if (Assists.polls < CONFIG.damageImpactIntervalPolls) {
        return;
      }
      Assists.polls = 0;
      var container = Panels.childById(root, "damageImpactInfo");
      if (!Panels.valid(container)) {
        return;
      }
      var live = [];
      for (var p = 0; p < Assists.credited.length; p++) {
        if (Panels.valid(Assists.credited[p])) {
          live.push(Assists.credited[p]);
        }
      }
      Assists.credited = live;
      var children = Panels.children(container);
      for (var i = 0; i < children.length; i++) {
        var child = children[i];
        if (!Panels.valid(child) || !Panels.hasClass(child, "assist")) {
          continue;
        }
        if (Assists.credited.indexOf(child) !== -1) {
          continue;
        }
        Assists.credited.push(child);
        Bridge.action("assist", { detection: "damage_impact_assist_class" });
      }
    }
  };

  var KillBanner = {
    lastKey: null,
    polls: 0,

    key: function (root) {
      var texts = [];
      Panels.labelTexts(root, texts);
      var hits = [];
      for (var i = 0; i < texts.length; i++) {
        for (var p = 0; p < CONFIG.killBannerPatterns.length; p++) {
          if (CONFIG.killBannerPatterns[p].test(texts[i])) {
            hits.push(texts[i]);
            break;
          }
        }
      }
      hits.sort();
      return hits.length > 0 ? hits.join("|") : null;
    },

    poll: function (root) {
      KillBanner.polls++;
      if (KillBanner.polls < CONFIG.killBannerIntervalPolls) {
        return;
      }
      KillBanner.polls = 0;
      var key = KillBanner.key(root);
      var previous = KillBanner.lastKey;
      KillBanner.lastKey = key;
      if (key !== null && key !== previous && Date.now() - Bridge.lastKillMs > CONFIG.killBannerCooldownMs) {
        Bridge.kill({ detection: "kill_banner_text" });
      }
    }
  };

  var RespawnTimer = {
    dead: false,

    poll: function (root) {
      var texts = [];
      Panels.labelTexts(root, texts);
      var timerDead = false;
      for (var i = 0; i < texts.length; i++) {
        if (/respawn/i.test(texts[i])) {
          timerDead = true;
          break;
        }
      }
      if (timerDead === RespawnTimer.dead) {
        return;
      }
      RespawnTimer.dead = timerDead;
      if (timerDead && Date.now() - Bridge.lastDeathMs > CONFIG.respawnDeathCooldownMs) {
        Bridge.death({ detection: "respawn_timer_label" });
      } else if (!timerDead) {
        Bridge.action("respawn", { detection: "respawn_timer_label" });
      }
    }
  };

  var Match = {
    context: $.GetContextPanel(),
    localPlayer: null,
    deathBaseline: false,
    wasDead: false,
    settlePolls: 0,

    findLocalPlayer: function () {
      if (Panels.valid(Match.localPlayer)) {
        return Match.localPlayer;
      }
      Match.localPlayer = null;
      Match.deathBaseline = false;
      KillStreak.reset();
      var panels = Match.context.FindChildrenWithClassTraverse("LocalPlayer");
      for (var i = 0; i < panels.length; i++) {
        if (panels[i].paneltype === "CitadelHudTopBarPlayer") {
          Match.localPlayer = panels[i];
          return Match.localPlayer;
        }
      }
      return null;
    },

    pollLiveness: function (player) {
      var isDead = Panels.hasClass(player, "Dead");
      var rebaseline = false;
      if (!Match.deathBaseline) {
        Match.wasDead = isDead;
        Match.deathBaseline = true;
        Match.settlePolls = CONFIG.baselineSettlePolls;
        rebaseline = true;
      } else if (Match.settlePolls > 0) {
        Match.settlePolls--;
        Match.wasDead = isDead;
      } else if (isDead !== Match.wasDead) {
        rebaseline = true;
        if (isDead) {
          Bridge.death({ detection: "top_bar_local_player_dead_class" });
        } else {
          Bridge.action("respawn", { detection: "top_bar_local_player_dead_class" });
        }
        Match.wasDead = isDead;
      }
      return { rebaseline: rebaseline, isDead: isDead, settling: Match.settlePolls > 0 };
    },

    poll: function () {
      if (!Panels.valid(Match.context)) {
        return;
      }
      var player = Match.findLocalPlayer();
      if (!player) {
        Abilities.reset();
        $.Schedule(CONFIG.pollIntervalSeconds, Match.poll);
        return;
      }
      var liveness = Match.pollLiveness(player);
      Abilities.poll(Match.context, liveness.rebaseline, liveness.isDead);
      KillStreak.poll(player, liveness.rebaseline, liveness.settling);
      var ancestor = Panels.root(Match.context);
      Assists.poll(ancestor);
      KillBanner.poll(ancestor);
      RespawnTimer.poll(ancestor);
      $.Schedule(CONFIG.pollIntervalSeconds, Match.poll);
    }
  };

  Bridge.send("hook_ready", { poll_interval_ms: CONFIG.pollIntervalSeconds * 1000 });
  Match.poll();
})();
