#!/usr/bin/env python3
"""Compare deux photographies de snapshot-programme.sh, par avance (jour - date de la plus récente).

Usage : backend/scripts/compare-programmes.py data/snapshots/A.tsv data/snapshots/B.tsv

Un couple (cinéma, jour) présent dans les deux est « modifié » si une séance (film + début)
est apparue ou a disparu. Les jours déjà passés dans B sont ignorés, ainsi que, pour le jour
même, les séances déjà commencées quand B a été pris (AlloCiné les retire en cours de journée ;
heure de B = date de modification du fichier, en heure locale comme `startsAt`).
"""

import sys
from collections import Counter, defaultdict
from datetime import date, datetime
from pathlib import Path


def load(path):
    pairs = defaultdict(Counter)
    for line in Path(path).read_text().splitlines():
        cinema, day, _id, movie, starts_at = line.split("\t")
        pairs[(cinema, day)]  # couple vide : présent, sans séance
        if movie:
            pairs[(cinema, day)][(movie, starts_at)] += 1
    return pairs


def main(old_path, new_path):
    old, new = load(old_path), load(new_path)
    taken = date.fromisoformat(Path(new_path).stem)
    taken_at = datetime.fromtimestamp(Path(new_path).stat().st_mtime).isoformat(timespec="seconds")
    by_lead = defaultdict(lambda: [0, 0, 0, 0, 0])  # couples, modifiés, ajouts, retraits, vides→pleins
    for key in old.keys() & new.keys():
        cinema, day = key
        lead = (date.fromisoformat(day) - taken).days
        if lead < 0:
            continue
        before, after = old[key], new[key]
        if lead == 0:
            before = Counter({k: n for k, n in before.items() if k[1] >= taken_at})
            after = Counter({k: n for k, n in after.items() if k[1] >= taken_at})
        added = sum((after - before).values())
        removed = sum((before - after).values())
        row = by_lead[lead]
        row[0] += 1
        if added or removed:
            row[1] += 1
        row[2] += added
        row[3] += removed
        if not old[key] and new[key]:
            row[4] += 1

    print(f"{Path(old_path).stem} → {Path(new_path).stem}")
    print("avance  couples  modifiés  ajouts  retraits  vides→programmés")
    for lead in sorted(by_lead):
        pairs, changed, added, removed, filled = by_lead[lead]
        print(f"J+{lead:<5} {pairs:>7}  {changed:>4} ({changed / pairs:>4.0%})  {added:>6}  {removed:>8}  {filled:>8}")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2])
