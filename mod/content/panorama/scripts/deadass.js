(function () {
  var LOG_PREFIX = "[DEADASS]";
  var MOD_VERSION = "0.1.0";
  var POLL_MS = 100;
  var sequence = 0;
  var sessionId = Date.now().toString(36) + "-" + Math.floor(Math.random() * 0x1000000).toString(36);

  var previous = {
    kills: null,
    deaths: null,
    assists: null,
    alive: null,
    ready: [null, null, null, null]
  };

  function localScoreboard() {
    var info = Game.GetLocalPlayerInfo();
    if (!info) {
      return null;
    }
    return {
      kills: info.player_kills || 0,
      deaths: info.player_deaths || 0,
      assists: info.player_assists || 0,
      alive: info.player_respawn_timer == null || info.player_respawn_timer <= 0
    };
  }

  function abilityReady(slot) {
    var playerId = Game.GetLocalPlayerID();
    var ability = Abilities.GetAbility(playerId, slot);
    if (ability == null || ability < 0) {
      return null;
    }
    return Abilities.IsCooldownReady(ability);
  }

  function emit(event, fields) {
    sequence += 1;
    var payload = {
      schema: 1,
      event: event,
      mod_version: MOD_VERSION,
      session_id: sessionId,
      sequence: sequence,
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

  function observeCounters(current) {
    if (previous.kills == null) {
      previous.kills = current.kills;
      previous.deaths = current.deaths;
      previous.assists = current.assists;
      previous.alive = current.alive;
      return;
    }
    for (var i = previous.kills; i < current.kills; i++) {
      emit("kill");
    }
    for (var j = previous.deaths; j < current.deaths; j++) {
      emit("death");
    }
    for (var k = previous.assists; k < current.assists; k++) {
      emit("assist");
    }
    if (!previous.alive && current.alive) {
      emit("respawn");
    }
    previous.kills = current.kills;
    previous.deaths = current.deaths;
    previous.assists = current.assists;
    previous.alive = current.alive;
  }

  function observeAbilities() {
    for (var slot = 0; slot < 4; slot++) {
      var ready = null;
      try {
        ready = abilityReady(slot);
      } catch (e) {
        ready = null;
      }
      if (ready == null) {
        continue;
      }
      if (previous.ready[slot] == null) {
        previous.ready[slot] = ready;
        continue;
      }
      if (!previous.ready[slot] && ready) {
        emit("ability_ready", { ability_slot: slot });
      }
      if (previous.ready[slot] && !ready) {
        emit("ability_used", { ability_slot: slot });
      }
      previous.ready[slot] = ready;
    }
  }

  function tick() {
    var scoreboard = localScoreboard();
    if (scoreboard) {
      observeCounters(scoreboard);
    }
    observeAbilities();
    $.Schedule(POLL_MS / 1000, tick);
  }

  emit("hook_ready", { poll_interval_ms: POLL_MS });
  $.Schedule(2, tick);
})();
