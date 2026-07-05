#!/usr/bin/env python3
"""Seed test KDS stations and routing rules into restaurant.db."""
import sqlite3, uuid, sys

DB = sys.argv[1] if len(sys.argv) > 1 else "restaurant.db"
conn = sqlite3.connect(DB)
conn.execute("PRAGMA foreign_keys = ON")

# Stations
# Insert order: boissons-01 first (no fallback), then grill-01 (depends on boissons-01),
# then assembly-01, expo-01, ready_board
conn.executemany(
    """INSERT OR REPLACE INTO kds_stations
       (id, name, role, output_type, printer_type, printer_address,
        fallback_station_id, active_in_profiles, sort_order, enabled)
       VALUES (?,?,?,?,?,?,?,?,?,?)""",
    [
        ("boissons-01", "Boissons",  "preparation", "screen",
         "file", "/tmp/print_boissons.txt", None,          '["normal","rush"]', 2, 1),
        ("grill-01",    "Grill",     "preparation", "screen",
         "file", "/tmp/print_grill.txt",    "boissons-01", '["normal","rush"]', 1, 1),
        ("assembly-01", "Assembly",  "assembly",    "screen",
         "file", "/tmp/print_assembly.txt", None,          '["normal","rush"]', 3, 1),
        ("expo-01",     "Expo",      "expo",        "screen",
         None,   None,                      None,          '["normal","rush"]', 4, 1),
        ("ready_board", "Prêt",      "ready_board", "screen",
         None,   None,                      None,          '["normal","rush"]', 5, 1),
    ]
)

# Routing rules for profile "normal"
# assembly and expo receive all orders via Phase 4 in dispatch_order (no routing rules needed)
conn.executemany(
    """INSERT OR REPLACE INTO kds_routing_rules
       (id, profile_id, rule_type, match_value, station_ids, priority)
       VALUES (?,?,?,?,?,?)""",
    [
        (str(uuid.uuid4()), "normal", "category", "food",  '["grill-01"]',    10),
        (str(uuid.uuid4()), "normal", "category", "drink", '["boissons-01"]', 10),
    ]
)

conn.commit()
conn.close()
print(f"Seeded {DB} — 5 stations (grill-01, boissons-01, assembly-01, expo-01, ready_board), 2 routing rules.")
