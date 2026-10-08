# Déploiement (étape 6)

Écrit par Claude le 2026-10-08. Prod : **https://cinemap.ethansaux.fr**, sur le VPS (Ubuntu 24.04, x86_64, 4 cœurs, 7,6 Go). `cinemap.eowinstudio.com` n'est plus servi.

## Vue d'ensemble

```text
navigateur ──HTTPS──► nginx (certbot)
                        ├─ /api/…   → proxy → 127.0.0.1:11174 ─ cinemap serve (systemd : cinemap.service)
                        ├─ /assets/ → fichiers hashés de Vite, cache 1 an
                        └─ le reste → /opt/cinemap/current/www, chemin inconnu → index.html

timers systemd (heure de Paris) :
  chaque nuit 4 h   cinemap-nightly : cinemap scrape, puis cinemap tmdb
  le mardi 2 h      cinemap-weekly  : cinemap import-cinemas (+ géocodage + CNC)

push sur main ──► GitHub Actions : fmt, clippy, tests, build release (Rust)
                                   test, check, lint, build (front)
                                   └─ tar par SSH → /opt/cinemap/releases/<sha>/
                                      activate.sh : bascule `current`, restart, /api/meta
                                      (échec → retour à la release précédente)
                                   └─ https://cinemap.ethansaux.fr/api/meta
```

Le binaire est autonome : migrations SQL et liste des départements sont compilées dedans. Le VPS n'a besoin ni de Rust, ni de Node, ni du dépôt git.

## Fichiers

| Dans le dépôt | Sur le VPS |
|---|---|
| `deploy/systemd/cinemap.service` | `/etc/systemd/system/cinemap.service` |
| `deploy/systemd/cinemap-nightly.{service,timer}` | `/etc/systemd/system/` |
| `deploy/systemd/cinemap-weekly.{service,timer}` | `/etc/systemd/system/` |
| `deploy/nginx/cinemap.conf` | `/etc/nginx/sites-available/cinemap` (+ lien dans `sites-enabled`) |
| `deploy/cinemap.env.example` | `/etc/cinemap/cinemap.env` (600, secrets) |
| `deploy/activate.sh` | copié dans chaque release par la CI |

```text
/opt/cinemap/
  releases/<sha>/cinemap        binaire (target/release/backend renommé)
  releases/<sha>/www/           frontend/dist
  releases/<sha>/activate.sh
  current -> releases/<sha>     lien basculé à chaque déploiement (5 releases gardées)
/var/lib/cinemap/cinemap.db     base SQLite (WAL), propriétaire cinemap
/etc/cinemap/cinemap.env        DATABASE_URL, TMDB_API_KEY, BIND_ADDR, RUST_LOG
```

L'API, le scrape et TMDB tournent sous l'utilisateur système **`cinemap`** (sans shell, sans home), et ne peuvent écrire que dans `/var/lib/cinemap` (`ProtectSystem=strict`). La CI se connecte en **root** (secret `SSH_USER`) : elle écrit dans `/opt/cinemap` et lance `systemctl restart cinemap` sans réglage particulier.

## Mise en place (une seule fois)

Les commandes VPS sont à lancer **en root**.

### 1. DNS

Chez le registrar de `ethansaux.fr` : un enregistrement **A** `cinemap` → IPv4 du VPS, et un **AAAA** → IPv6 du VPS s'il en a une (sinon, pas d'AAAA : Let's Encrypt essaierait l'IPv6 et échouerait).

```bash
dig +short cinemap.ethansaux.fr A        # depuis ta machine : doit afficher l'IP du VPS
dig +short cinemap.ethansaux.fr AAAA
```

### 2. Arrêter et retirer l'ancien service Python

```bash
systemctl disable --now cinemap
rm /etc/systemd/system/cinemap.service
systemctl daemon-reload
mv /opt/cinemap /opt/cinemap-old                     # supprimé à l'étape 9

# Ancien site nginx (cinemap.eowinstudio.com) et son certificat
ls -l /etc/nginx/sites-enabled/cinemap               # lien vers sites-available ou fichier ?
rm /etc/nginx/sites-enabled/cinemap
mv /etc/nginx/sites-available/cinemap /opt/cinemap-old/nginx-eowinstudio.conf 2>/dev/null
nginx -t && systemctl reload nginx
certbot delete --cert-name cinemap.eowinstudio.com   # `certbot certificates` pour voir le nom exact

# L'ancienne doc proposait un cron pour le scraper Python
crontab -l 2>/dev/null | grep -n cinemap; crontab -u www-data -l 2>/dev/null | grep -n cinemap
```

### 3. Utilisateur, dossiers, secrets

Depuis ta machine, à la racine du dépôt (branche `rewrite` ou `main`) :

```bash
scp -r deploy <ton-user>@<vps>:/tmp/cinemap-deploy
```

Sur le VPS :

```bash
useradd --system --no-create-home --shell /usr/sbin/nologin cinemap
install -d /opt/cinemap/releases

install -d -m 755 /etc/cinemap
install -m 600 /tmp/cinemap-deploy/cinemap.env.example /etc/cinemap/cinemap.env
nano /etc/cinemap/cinemap.env                         # TMDB_API_KEY = celle de backend/.env

ss -ltnp | grep ':11174 '                             # doit être vide (port libre)
```

### 4. systemd

```bash
cp /tmp/cinemap-deploy/systemd/cinemap* /etc/systemd/system/
systemctl daemon-reload
systemctl enable cinemap                              # démarré par le premier déploiement
systemd-analyze verify /etc/systemd/system/cinemap*.service   # aucune sortie = OK
systemd-analyze calendar '*-*-* 04:00:00 Europe/Paris' 'Tue *-*-* 02:00:00 Europe/Paris'
```

Les timers sont activés à l'étape 8, une fois la base remplie.

### 5. nginx et HTTPS

```bash
cp /tmp/cinemap-deploy/nginx/cinemap.conf /etc/nginx/sites-available/cinemap
ln -s /etc/nginx/sites-available/cinemap /etc/nginx/sites-enabled/cinemap
nginx -t && systemctl reload nginx
certbot --nginx -d cinemap.ethansaux.fr               # ajoute le bloc 443 et la redirection HTTP → HTTPS
nginx -t && systemctl reload nginx
curl -sI http://cinemap.ethansaux.fr | head -3        # 301 vers https
```

`certbot --nginx` modifie le fichier **installé** : `deploy/nginx/cinemap.conf` reste la version HTTP de départ. Pour changer la config plus tard, édite le fichier du VPS (et reporte la modification dans le dépôt).

### 6. Droits de la CI

Rien à faire : `SSH_USER` est `root`. Si un jour la CI passe par un utilisateur dédié (moins de pouvoir si la clé fuit), il faudra lui donner `/opt/cinemap` (`chown -R`) et, via `visudo -f /etc/sudoers.d/cinemap` :

```text
<user-ci> ALL=(root) NOPASSWD: /usr/bin/systemctl restart cinemap, /usr/bin/journalctl -u cinemap -n 30 --no-pager
```

Les secrets `SSH_HOST`, `SSH_USER` et `SSH_PRIVATE_KEY` existants ne changent pas.

### 7. Premier déploiement

Merge `rewrite` → `main` et push (le seul moyen d'obtenir une release : elle est construite par la CI). Suivre le run dans l'onglet **Actions**. À la fin :

```bash
systemctl status cinemap --no-pager
journalctl -u cinemap -n 20 --no-pager                # « API démarrée addr=127.0.0.1:11174 »
curl -s https://cinemap.ethansaux.fr/api/meta         # base vide : cinema_count = 0
```

Le site s'affiche déjà, sans données.

### 8. Remplir la base, puis activer les timers

Le premier import fait tous les départements et géocode **tous** les cinémas (≈ 7 min de scraping AlloCiné, plus le géocodage et le CNC). Le scrape complet est estimé à ~1 h (jamais mesuré en entier) ; TMDB, quelques minutes de plus (≈ 3 000 films au lieu de 344 pour Paris).

```bash
systemctl start --no-block cinemap-weekly
journalctl -u cinemap-weekly -f                       # Ctrl-C quand « Finished » apparaît
systemctl start --no-block cinemap-nightly
journalctl -u cinemap-nightly -f

systemctl enable --now cinemap-nightly.timer cinemap-weekly.timer
systemctl list-timers 'cinemap*'
```

`systemctl start` sans `--no-block` attend la fin d'un service `oneshot` (~1 h) : tu peux aussi le lancer dans un `tmux`.

Vérifier :

```bash
apt install sqlite3                                   # si absent
sqlite3 /var/lib/cinemap/cinemap.db "select kind, started_at, finished_at, ok_count, error_count from scrape_runs order by id desc limit 3;"
curl -s https://cinemap.ethansaux.fr/api/meta
systemctl show cinemap-nightly -p ExecMainStartTimestamp -p ExecMainExitTimestamp   # durée réelle
```

Puis ouvrir le site sur ton téléphone (installation PWA, géolocalisation : l'HTTPS est obligatoire pour les deux).

### 9. Nettoyage

Quand tout va bien : `rm -rf /opt/cinemap-old`, et supprimer l'enregistrement DNS `cinemap.eowinstudio.com` s'il ne sert plus.

## Au quotidien

```bash
journalctl -u cinemap -f                              # logs de l'API (RUST_LOG dans cinemap.env)
journalctl -u cinemap-nightly --since today           # dernier scrape
systemctl list-timers 'cinemap*'                      # prochains runs
systemctl start --no-block cinemap-nightly            # relancer un scrape à la main
```

**Lancer une commande à la main** (avec les mêmes droits et variables que les services) :

```bash
systemd-run --pty --wait --collect --uid=cinemap --gid=cinemap \
  -p EnvironmentFile=/etc/cinemap/cinemap.env -p WorkingDirectory=/var/lib/cinemap \
  /opt/cinemap/current/cinemap scrape --department 75
```

**Redéployer sans commit** : onglet Actions → CI → *Run workflow* sur `main`.

**Revenir à une release précédente** :

```bash
ls -lt /opt/cinemap/releases/
ln -sfn /opt/cinemap/releases/<sha> /opt/cinemap/current && systemctl restart cinemap
```

**Changer une migration ou une requête `sqlx::query!`** : relancer `cargo sqlx prepare -- --all-targets` dans `backend/` et versionner `backend/.sqlx/`. Sinon la CI (`SQLX_OFFLINE=true`, sans base) échoue avec « no cached data for this query ».

## Ce qu'il reste à mesurer sur le VPS

À noter dans `SOBRIETE.md` après le premier run complet :
- durée réelle du scrape France entière et de TMDB (`systemctl show … ExecMainStartTimestamp/ExecMainExitTimestamp`) ;
- mémoire de l'API au repos : `systemctl show cinemap -p MemoryCurrent` ;
- pic mémoire du scrape : `systemctl show cinemap-nightly -p MemoryPeak` (après le run) ;
- taille de la base : `du -h /var/lib/cinemap/`.

## Écarts au plan

- **Domaine** : `cinemap.ethansaux.fr` au lieu de `cinemap.eowinstudio.com` (décision du 2026-10-08), l'ancien domaine est abandonné sans redirection.
- **Timer hebdomadaire** : une seule commande, `import-cinemas`, qui enchaîne déjà le géocodage des nouvelles adresses et le CNC. `geocode` et `enrich-cnc` restent disponibles à la main.
- **Merge avant la migration** : le premier déploiement passe par la CI (c'est elle qui construit le binaire), donc `main` est mergée avant que la base de prod soit remplie. Le site est vide pendant ~1 h, accepté car il n'est pas encore utilisé.
- **Releases versionnées** (`releases/<sha>` + lien `current`) au lieu d'écraser les fichiers : retour arrière automatique si `/api/meta` ne répond pas après le restart.
- **CI** : un seul workflow (`ci.yml`) remplace `deploy.yml` ; plus d'`appleboy/ssh-action`, un simple `ssh` + `tar`. La clé d'hôte du VPS est relevée par `ssh-keyscan` à chaque run (confiance au premier usage, pas d'épinglage).
- **Backend** : `serve` gère SIGTERM (`shutdown_signal`), et les logs n'ont plus de couleurs ANSI hors terminal (sinon journald affiche des `\e[32m`).
- **Port 11174** au lieu de 3000 : 3000 est un port très demandé sur un VPS qui héberge déjà d'autres sites.
- Pas de sauvegarde de la base : tout se régénère avec `cinemap-weekly` puis `cinemap-nightly` (~1 h 15).

## Vérifié

- `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo test` (152 tests) avec `SQLX_OFFLINE=true` ; build d'une copie du dépôt **sans `.env` ni base** : les 25 requêtes de `.sqlx/` suffisent.
- `serve` reçoit SIGTERM → « Arrêt demandé », « API arrêtée », code de sortie 0.
- Front : 15 tests, `svelte-check` 0 erreur, lint, build.
- `activate.sh` en simulation (faux `systemctl`, faux serveur) : 6 releases → 5 gardées ; une release qui ne répond pas → retour à la précédente, code 1 ; une release incomplète → refusée.

Pas vérifié : la config nginx (pas de nginx en local : `nginx -t` à l'étape 5), les unités systemd (`systemd-analyze verify` à l'étape 4), le workflow lui-même (premier run au push).

## Questions pour toi

1. **systemd** : `cinemap-nightly.service` a deux lignes `ExecStart=`. Que se passe-t-il si le scrape se termine avec une erreur ? Et pourquoi `Type=oneshot` plutôt que le `Type=simple` (implicite) de `cinemap.service` ?
2. **nginx** : pourquoi `/film/123` renvoie-t-il `index.html` alors que `/assets/inconnu.js` renvoie une 404 ? Qu'est-ce qui casserait si `/assets/` tombait lui aussi sur `index.html` ?
3. **CI** : `deploy` dépend de `backend` et `frontend` (`needs`). Si `cargo clippy` échoue sur `main`, qu'est-ce qui est déployé ? Et pourquoi le binaire est-il construit sur `ubuntu-24.04` plutôt que sur `ubuntu-latest` ?
