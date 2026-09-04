(function () {
  var COMPANION_URL = "http://127.0.0.1:24681/event";
  var POLL_MS = 100;
  var sequence = 0;

  var previous = {
    kills: null,
    deaths: null,
    assists: null,
    alive: null,
    ready: [null, null, null, null]
  };

  function wallTimeMs() {
    return Date.now();
  }

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

  function emit(kind) {
    sequence += 1;
    var payload = {
      sequence: sequence,
      wall_time_ms: wallTimeMs(),
      source: "mod",
      kind: kind
    };
    $.AsyncWebRequest(COMPANION_URL, {
      type: "POST",
      data: payload,
      complete: function () {}
    });
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
      emit({ type: "kill" });
    }
    for (var j = previous.deaths; j < current.deaths; j++) {
      emit({ type: "death" });
    }
    for (var k = previous.assists; k < current.assists; k++) {
      emit({ type: "assist" });
    }
    if (!previous.alive && current.alive) {
      emit({ type: "respawn" });
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
        emit({ type: "ability_ready", slot: slot });
      }
      if (previous.ready[slot] && !ready) {
        emit({ type: "ability_used", slot: slot });
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

  $.Schedule(2, tick);
})();
