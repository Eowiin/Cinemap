#!/bin/bash
# Photographie des programmes AlloCiné d'un échantillon fixe de cinémas, de J à J+6, pages
# fraîches (paramètre unique : contourne le cache Cloudflare, qui sert des copies de
# plusieurs heures). Sert à mesurer en local, sans passer par la prod, à quelle avance les
# programmes changent : lancer un jour, relancer le lendemain, puis compare-programmes.py.
#
# Usage (depuis la racine du dépôt) : backend/scripts/snapshot-programme.sh [nb_cinemas]
# Écrit data/snapshots/<date>.tsv (cinema, date, id séance, film, début). ~1 req/s.
set -euo pipefail

DB=backend/database.db
DIR=data/snapshots
SAMPLE=$DIR/cinemas.txt
N=${1:-40}
UA="Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"

mkdir -p "$DIR"
# Même échantillon d'une fois sur l'autre, sinon les photographies ne se comparent pas.
if [ ! -s "$SAMPLE" ]; then
  sqlite3 "$DB" "SELECT id FROM visible_cinemas
    WHERE department IN ('75','77','78','91','92','93','94','95')
    ORDER BY random() LIMIT $N" > "$SAMPLE"
fi

today=$(date +%Y-%m-%d)
out=$DIR/$today.tsv
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
: > "$out.part"
requests=0
errors=0

while read -r cinema; do
  for i in 0 1 2 3 4 5 6; do
    day=$(date -v+${i}d +%Y-%m-%d)
    page=1
    pages=1
    while [ "$page" -le "$pages" ]; do
      path=""
      [ "$page" -gt 1 ] && path="p-$page/"
      status=$(curl -s -A "$UA" -o "$tmp" -w '%{http_code}' \
        "https://www.allocine.fr/_/showtimes/theater-$cinema/d-$day/$path?_=$(date +%s)$RANDOM")
      requests=$((requests + 1))
      sleep 1
      if [ "$status" != 200 ]; then
        echo "HTTP $status : $cinema $day page $page" >&2
        errors=$((errors + 1))
        break
      fi
      pages=$(jq '.pagination.totalPages // 1' "$tmp")
      # Jour vide : AlloCiné répond `error: true` sans résultats, on note le couple vide.
      jq -r --arg c "$cinema" --arg d "$day" '
        if (.results | length) == 0 then [$c, $d, "", "", ""] | @tsv
        else .results[] | .movie.internalId as $m
          | .showtimes[][] | [$c, $d, (.internalId | tostring), ($m | tostring), .startsAt] | @tsv
        end' "$tmp" >> "$out.part"
      page=$((page + 1))
    done
  done
done < "$SAMPLE"

mv "$out.part" "$out"
echo "$out : $requests requêtes, $errors erreurs"
