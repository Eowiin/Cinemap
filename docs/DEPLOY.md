# Déploiement en production — Cinemap IDF

## Est-ce prêt pour la prod ?

**Oui, pour un usage personnel/petit public.** L'application est fonctionnelle telle quelle.  
Il manque juste le "packaging" côté serveur (service systemd + reverse proxy nginx) pour qu'elle tourne en permanence et soit accessible depuis l'extérieur.

---

## Ce qui est déjà bon

| Point | État |
|---|---|
| API FastAPI + frontend statique servis ensemble | ✅ |
| SQLite (lecture seule, usage faible) | ✅ |
| Headers CORS et no-cache | ✅ |
| CSS/JS dans des fichiers séparés | ✅ |
| Scraper avec `--days N` | ✅ |

---

## Prérequis sur le VPS

```bash
# Python 3.11+
python3 --version

# nginx
sudo apt install nginx

# certbot (HTTPS)
sudo apt install certbot python3-certbot-nginx
```

---

## Installation

```bash
# 1. Cloner le projet
git clone <ton-repo> /opt/cinemap
cd /opt/cinemap

# 2. Environnement virtuel
python3 -m venv .venv
.venv/bin/pip install -r backend/requirements.txt

# 3. Initialiser la base et scraper les données
.venv/bin/python -m backend.scraper.cinemas      # cinémas IDF
.venv/bin/python -m backend.scraper.allocine_match  # matching AlloCiné
.venv/bin/python -m backend.scraper.showtimes --days 7  # séances
```

---

## Service systemd

Crée le fichier `/etc/systemd/system/cinemap.service` :

```ini
[Unit]
Description=Cinemap IDF
After=network.target

[Service]
WorkingDirectory=/opt/cinemap
ExecStart=/opt/cinemap/.venv/bin/uvicorn backend.api.main:app --host 127.0.0.1 --port 8000
Restart=always
User=www-data
Group=www-data

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable cinemap
sudo systemctl start cinemap

# Vérifier que ça tourne
sudo systemctl status cinemap
```

---

## Reverse proxy nginx

Crée `/etc/nginx/sites-available/cinemap` :

```nginx
server {
    listen 80;
    server_name ton-domaine.com;

    location / {
        proxy_pass         http://127.0.0.1:8000;
        proxy_set_header   Host $host;
        proxy_set_header   X-Real-IP $remote_addr;
        proxy_set_header   X-Forwarded-For $proxy_add_x_forwarded_for;
    }
}
```

```bash
sudo ln -s /etc/nginx/sites-available/cinemap /etc/nginx/sites-enabled/
sudo nginx -t
sudo systemctl reload nginx

# HTTPS (remplace ton-domaine.com)
sudo certbot --nginx -d ton-domaine.com
```

---

## Cron — mise à jour des séances

Les cinémas français sortent les films le **mercredi**. Le scraper tourne une fois par semaine ce jour-là, 7 jours d'avance.

```bash
# Ajouter au crontab : crontab -e
0 6 * * 3 cd /opt/cinemap && .venv/bin/python -m backend.scraper.showtimes --days 7 >> /var/log/cinemap-scraper.log 2>&1
```

- `0 6 * * 3` = tous les mercredis à 6h00
- `--days 7` = couvre mercredi → mardi suivant
- Les logs vont dans `/var/log/cinemap-scraper.log`

Si tu veux aussi rafraîchir le jour même (mercredi en soirée par ex.) :

```bash
0 20 * * 3 cd /opt/cinemap && .venv/bin/python -m backend.scraper.showtimes --days 7 >> /var/log/cinemap-scraper.log 2>&1
```

---

## Déploiement continu (GitHub Actions)

Le fichier `.github/workflows/deploy.yml` déclenche automatiquement un déploiement à chaque push sur `main` : il se connecte au VPS en SSH, fait `git pull` et redémarre le service.

### 1. Créer une clé SSH dédiée (sur ta machine locale)

```bash
ssh-keygen -t ed25519 -C "github-actions-cinemap" -f ~/.ssh/cinemap_deploy
# Ne pas mettre de passphrase
```

### 2. Autoriser cette clé sur le VPS

```bash
ssh-copy-id -i ~/.ssh/cinemap_deploy.pub user@ton-vps
```

### 3. Ajouter les secrets dans GitHub

Settings → Secrets and variables → Actions :

| Secret | Valeur |
|---|---|
| `SSH_HOST` | IP ou domaine du VPS |
| `SSH_USER` | ton user sur le VPS |
| `SSH_PRIVATE_KEY` | contenu de `~/.ssh/cinemap_deploy` |

### 4. Autoriser le restart sans mot de passe

Sur le VPS, via `sudo visudo` :

```
ton-user ALL=(ALL) NOPASSWD: /bin/systemctl restart cinemap
```

> **Note** : GitHub Actions ne fait que mettre à jour un repo déjà en place (`git pull`). Le premier déploiement se fait obligatoirement à la main en suivant la section "Installation" ci-dessus.

---

## Vérifications post-déploiement

```bash
# L'API répond
curl http://localhost:8000/api/today

# Les cinémas sont chargés
curl http://localhost:8000/api/cinemas | python3 -c "import sys,json; d=json.load(sys.stdin); print(len(d), 'cinémas')"

# Les logs du service
sudo journalctl -u cinemap -f
```

---

## Limites à connaître

- **SQLite** : parfait pour ce projet (lecture seule, un seul serveur). Pas adapté si tu as des écritures concurrentes.
- **AlloCiné** : le scraper dépend de leur API non officielle. Si le format change, il faudra adapter `backend/scraper/showtimes.py`.
- **Pas d'auth** : l'API est publique en lecture. C'est voulu pour un site de consultation.
