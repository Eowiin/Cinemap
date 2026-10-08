#!/usr/bin/env bash
# Active une release déjà copiée dans /opt/cinemap/releases/<sha>/ (lancé par la CI en SSH).
# Bascule le lien `current`, redémarre l'API, vérifie /api/meta ; en cas d'échec,
# revient à la release précédente. Garde les 5 dernières releases.
set -euo pipefail

SHA="${1:?usage: activate.sh <sha>}"
BASE=/opt/cinemap
RELEASE="$BASE/releases/$SHA"
# Port de BIND_ADDR (/etc/cinemap/cinemap.env).
HEALTH_URL=http://127.0.0.1:11174/api/meta
KEEP=5

[[ -x "$RELEASE/cinemap" && -f "$RELEASE/www/index.html" ]] || {
    echo "Release incomplète : $RELEASE" >&2
    exit 1
}

PREVIOUS=$(readlink "$BASE/current" || true)

switch_to() {
    # ln + mv : le remplacement du lien est atomique, nginx ne voit jamais de lien absent.
    ln -sfn "$1" "$BASE/current.tmp"
    mv -T "$BASE/current.tmp" "$BASE/current"
    sudo -n systemctl restart cinemap
}

healthy() {
    for _ in $(seq 1 20); do
        curl -fs -o /dev/null --max-time 2 "$HEALTH_URL" && return 0
        sleep 0.5
    done
    return 1
}

switch_to "$RELEASE"
if healthy; then
    echo "Release $SHA active"
else
    echo "Health check en échec pour $SHA" >&2
    sudo -n journalctl -u cinemap -n 30 --no-pager >&2 || true
    if [[ -n "$PREVIOUS" ]]; then
        echo "Retour à $PREVIOUS" >&2
        switch_to "$PREVIOUS"
    fi
    exit 1
fi

# Les plus récentes d'abord ; jamais la release active.
ls -1dt "$BASE"/releases/*/ | tail -n +$((KEEP + 1)) | while read -r old; do
    [[ "${old%/}" == "$RELEASE" ]] || rm -rf -- "$old"
done
