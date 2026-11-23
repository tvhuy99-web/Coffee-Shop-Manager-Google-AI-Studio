#!/usr/bin/env python3
"""Integrity contract for Phase-2 England development data.

No statement about real current fixtures or staff is made by passing this test.
"""
import collections
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "app/src/main/assets/england_v2"

def load(name):
    with (DATA / name).open(encoding="utf-8") as file:
        return json.load(file)

def require(ok, msg):
    if not ok:
        raise AssertionError(msg)

def main():
    manifest = load("manifest.json")
    clubs = load("clubs.json")["clubs"]
    require(manifest["counts"]["clubs"] == 232 and len(clubs) == 232, "232 clubs")
    require(len({c["id"] for c in clubs}) == 232, "club ID collision")
    require(len({c["name"] for c in clubs}) == 232, "duplicate club names")
    club_ids = {c["id"] for c in clubs}
    teams_by_id = {c["id"]: c for c in clubs}
    require(all(c["source_league_id"].startswith("ENG_") for c in clubs), "non-England league present")
    require(not (DATA.parent / "match_lab/epl20_original.json").exists(), "old ENG1 asset still present")
    require(manifest["provenance"]["fixtures"].startswith("entirely generated"), "fixture provenance lost")
    require(manifest["provenance"]["staff"] == "fully fictional", "staff provenance lost")
    assert sum(g["team_count"] for g in manifest["divisions"]) == 232
    all_player_ids, all_staff_ids, all_youth_ids, all_match_ids = set(), set(), set(), set()
    counts = collections.Counter()
    attribute_counts = {"technical": 13, "mental": 14, "physical": 8, "goalkeeping": 9, "hidden": 10}
    for d in manifest["divisions"]:
        doc = load(d["data_file"])
        league = d["id"]
        members = [c["id"] for c in clubs if c["league_id"] == league]
        require(len(members) == d["team_count"], f"member count {league}")
        require(set(doc["clubs"]) == set(members), f"member keys {league}")
        n = len(members)
        require(n >= 17 and n <= 24, f"league size {league}")
        seen = set()
        for match in doc["fixtures"]:
            require(match["competition_id"] == league, f"wrong match league {league}")
            home, away = match["home_club_id"], match["away_club_id"]
            require(home != away and home in members and away in members, "match has outsider or self-game")
            require((home, away) not in seen, f"fixture repeated {league}")
            require(match["status"] == "simulated_not_official", "fixture claims official source")
            require(match["id"] not in all_match_ids, "match ID collision")
            seen.add((home, away))
            all_match_ids.add(match["id"])
        require(len(seen) == n*(n-1), f"missing round robin match in {league}")
        for club in members:
            roster = [p for p in doc["players"] if p["club_id"] == club]
            require(len(roster) >= 24 and sum(p["position"] == "GK" for p in roster) >= 3, f"roster/GK: {club}")
            require(len([p for p in doc["staff"] if p["club_id"] == club]) == 20, f"staff roles: {club}")
            require(len([p for p in doc["youth"] if p["club_id"] == club]) == 12, f"youth team: {club}")
        for section, tracking in [("players", all_player_ids),("youth", all_youth_ids)]:
            for p in doc[section]:
                require(p["id"] not in tracking and p["club_id"] in club_ids, f"{section} ID/club collision")
                tracking.add(p["id"])
                attributes = p["simulation"]["attributes"]
                require(set(attributes) == set(attribute_counts), "missing attribute group")
                for group, expected in attribute_counts.items():
                    values = attributes[group]
                    require(len(values) == expected, f"attribute count {group}")
                    require(all(type(v) is int and 1 <= v <= 20 for v in values.values()), f"attribute scale {group}")
        for s in doc["staff"]:
            require(s["id"] not in all_staff_ids and s["club_id"] in club_ids, "duplicate staff ID/club")
            require(s["synthetic_identity"] is True, "staff incorrectly represented as factual")
            all_staff_ids.add(s["id"])
        counts["players"] += len(doc["players"])
        counts["youth"] += len(doc["youth"])
        counts["staff"] += len(doc["staff"])
        counts["fixtures"] += len(doc["fixtures"])
        counts["source_named_players"] += sum(p["record_origin"] == "PingFootball_V97_source_identity" for p in doc["players"])
        counts["synthetic_first_team"] += sum(p["synthetic_identity"] for p in doc["players"])
    for key, actual in counts.items():
        require(actual == manifest["counts"][key], f"manifest count mismatch {key}: {actual}")
    operations = load("club_operations.json")["records"]
    require(len(operations) == 232 and {r["club_id"] for r in operations} == club_ids,
            "club management records missing")
    for rec in operations:
        require(rec["record_origin"] == "fictional_club_operation_defaults", "management provenance missing")
        require(rec["stadium"]["verified_name"] is False, "invented official stadium name")
        for key in ["finance", "sponsorship", "board", "training", "scouting", "academy",
                    "medical", "contracts", "discipline", "infrastructure"]:
            require(key in rec, f"missing club system {key}: {rec['club_id']}")
    rules = load("competition_rules.json")
    require(len(rules["cup_competitions"]) == 5, "cup structure missing")
    require(all(not c["official_fixture_records"] and not c["fixtures"] for c in rules["cup_competitions"]),
            "cups with invented official fixtures")
    print("PASS England v2 database integrity:",dict(counts),"clubs",len(clubs),"divisions",len(manifest["divisions"]))
    print("NOTICE: source 2025, 54 attributes/finance and staff simulated, fixtures are NOT official, native Rust remains 20-attr projection.")

if __name__ == "__main__":
    try:main()
    except Exception as exc:
        print("FAIL England v2 database:",exc,file=sys.stderr)
        raise
