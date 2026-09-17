"""Generate deadass-offsets.toml from a dezlock-dump output directory.

After a game update, refresh every offset the DLL uses without rebuilding:

    # in the game (needs admin), from the dezlock-dump bin dir:
    dezlock-dump.exe --all
    # then, from the deadass repo:
    python scripts/gen_offsets.py <dezlock-dump>/bin/schema-dump/deadlock
    # -> prints a deadass-offsets.toml; save it next to deadass_dll.dll
"""

import re
import sys
from pathlib import Path

HEX = r"0x([0-9A-Fa-f]+)"


def parse_int(text):
    return int(text, 16)


def read_text(path):
    return Path(path).read_text(encoding="utf-8", errors="replace")


def globals_from(dump_dir):
    """Schema globals from _globals.txt (both [schema] and bare entries)."""
    text = read_text(dump_dir / "_globals.txt")
    out = {}
    for match in re.finditer(
        rf"^# (\w+) @ client\.dll\+{HEX} \(pointer\)", text, re.MULTILINE
    ):
        out.setdefault(match.group(1), parse_int(match.group(2)))
    for match in re.finditer(rf"^client\.dll::(\w+) = {HEX} \(pointer\)", text, re.MULTILINE):
        out.setdefault(match.group(1), parse_int(match.group(2)))
    return out


def all_sections(dump_dir):
    """(class name, chain list, body) for every entity class tree."""
    text = read_text(dump_dir / "_entity-paths.txt")
    headers = list(re.finditer(r"^# (\w+) \(size=\w+\)\n(?:# chain: ([^\n]+)\n)?", text, re.MULTILINE))
    sections = []
    for i, match in enumerate(headers):
        end = headers[i + 1].start() if i + 1 < len(headers) else len(text)
        chain = [name.strip() for name in match.group(2).split("->")] if match.group(2) else []
        sections.append((match.group(1), chain, text[match.end():end]))
    return sections


def component_pair_from(body):
    match = re.search(r"\+(0x[0-9A-Fa-f]+)\s+m_CCitadelAbilityComponent", body)
    if not match:
        return None
    component = parse_int(match.group(1))
    abilities = re.search(r"\+(0x[0-9A-Fa-f]+)\s+m_vecAbilities\s", body[match.end():])
    if not abilities:
        return None
    return component, parse_int(abilities.group(1))


def ability_component_from(dump_dir, pawn_chain):
    """Component+abilities offsets, searched up the pawn's inheritance chain.

    The component sits on an ancestor class section, not the pawn's own tree.
    Walk the chain; if no ancestor's section carries it, fall back to the most
    common component offset across all classes (the shared unit ancestor).
    """
    sections = {name: body for name, _, body in all_sections(dump_dir)}
    for ancestor in pawn_chain[1:]:
        body = sections.get(ancestor)
        if body is None:
            continue
        pair = component_pair_from(body)
        if pair:
            return {"pawn_ability_component": pair[0], "abilities_vector": pair[0] + pair[1]}
    # No guesswork beyond the chain: a wrong abilities offset reads garbage
    # ability entities, so emit nothing and let the baked defaults stand.
    return {}


def pawn_fields_from(dump_dir):
    """Pawn field offsets from the C_CitadelPlayerPawn tree in _entity-paths.txt."""
    section = next(
        (item for item in all_sections(dump_dir) if item[0] == "C_CitadelPlayerPawn"),
        None,
    )
    if not section:
        return {}
    _, chain, body = section
    fields = {}
    for name, key in [
        ("m_iHealth", "pawn_health"),
        ("m_iMaxHealth", "pawn_max_health"),
        ("m_lifeState", "pawn_life_state"),
        ("m_iTeamNum", "pawn_team"),
    ]:
        match = re.search(rf"\+(0x[0-9A-Fa-f]+)\s+{name}\s", body)
        if match:
            fields[key] = parse_int(match.group(1))
    # The pawn's own tree carries the ability component (possibly deep in the
    # embedded-component tail of the section); only fall back to the
    # inheritance chain if it doesn't.
    pair = component_pair_from(body)
    if not pair:
        fields.update(ability_component_from(dump_dir, chain))
    else:
        fields["pawn_ability_component"] = pair[0]
        fields["abilities_vector"] = pair[0] + pair[1]
    return fields


def ability_fields_from(dump_dir):
    """C_CitadelBaseAbility field offsets from client.txt."""
    text = read_text(dump_dir / "client.txt")
    fields = {}
    for name, key in [
        ("m_bChanneling", "ability_channeling"),
        ("m_flCooldownStart", "ability_cooldown_start"),
        ("m_flCooldownEnd", "ability_cooldown_end"),
        ("m_eAbilitySlot", "ability_slot"),
        ("m_iRemainingCharges", "ability_charges"),
    ]:
        match = re.search(rf"^C_CitadelBaseAbility\.{name} = (0x[0-9A-Fa-f]+)", text, re.MULTILINE)
        if match:
            fields[key] = parse_int(match.group(1))
    return fields


def main():
    if len(sys.argv) != 2:
        raise SystemExit(__doc__)
    dump_dir = Path(sys.argv[1])
    globals_ = globals_from(dump_dir)
    pawn = pawn_fields_from(dump_dir)
    ability = ability_fields_from(dump_dir)

    missing = [k for k in ("C_CitadelPlayerPawn", "CGameEntitySystem") if k not in globals_]
    missing += [k for k in ("pawn_health", "pawn_life_state", "abilities_vector") if k not in pawn]
    if missing:
        raise SystemExit(f"dump is missing {missing}; run dezlock-dump.exe --all and re-check")

    lines = [
        "# deadass-offsets.toml — generated by scripts/gen_offsets.py",
        f"# from dezlock dump: {dump_dir}",
        "",
        "[dll]",
        "port = 24680",
        "poll_interval_ms = 33",
        "",
        "[globals]",
        f'local_pawn = "{globals_["C_CitadelPlayerPawn"]:#x}"',
        f'entity_system = "{globals_["CGameEntitySystem"]:#x}"',
        "",
        "[entity_list]",
        'chunk_array = "0x10"',
        "chunk_size = 512",
        f'stride = "0x{pawn.get("entity_stride", 0x70):x}"',
        "",
        "[pawn]",
    ]
    for key, name in [
        ("pawn_health", "health"),
        ("pawn_max_health", "max_health"),
        ("pawn_life_state", "life_state"),
        ("pawn_team", "team"),
    ]:
        if key in pawn:
            lines.append(f'{name} = "{pawn[key]:#x}"')
    if "pawn_ability_component" in pawn:
        lines.append(f'ability_component = "{pawn["pawn_ability_component"]:#x}"')
    if "abilities_vector" in pawn:
        lines.append(f'abilities = "{pawn["abilities_vector"]:#x}"')
    lines += ["", "[ability]"]
    for key in ("ability_channeling", "ability_cooldown_start", "ability_cooldown_end",
                "ability_slot", "ability_charges"):
        if key in ability:
            lines.append(f'{key} = "{ability[key]:#x}"')
    print("\n".join(lines))


if __name__ == "__main__":
    main()
