// ── Thème ──
const root = document.documentElement;
let theme  = localStorage.getItem('theme') || 'dark';
const TILES = {
  dark:  'https://{s}.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}{r}.png',
  light: 'https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}{r}.png',
};
let tileLayer = null;

function setTheme(t) {
  theme = t;
  root.dataset.theme = t;
  document.getElementById('theme-btn').textContent = t === 'dark' ? '☀️' : '🌙';
  localStorage.setItem('theme', t);
  if (tileLayer) tileLayer.setUrl(TILES[t]);
  refreshMarkers();
}
document.getElementById('theme-btn').addEventListener('click', () => {
  setTheme(theme === 'dark' ? 'light' : 'dark');
});

// ── Map ──
const canvasRenderer = L.canvas({ padding: 0.5 });

const map = L.map('map', {
  zoomControl:         false,
  renderer:            canvasRenderer,
  zoomSnap:            1,
  zoomDelta:           1,
  wheelPxPerZoomLevel: 40,
  inertia:             true,
  inertiaDeceleration: 3000,
  inertiaMaxSpeed:     2000,
  easeLinearity:       0.2,
}).setView([46.5, 2.3], 6);

tileLayer = L.tileLayer(TILES[theme], {
  attribution: '© <a href="https://osm.org/copyright">OSM</a> © <a href="https://carto.com/">CARTO</a>',
  subdomains: 'abcd',
  maxZoom: 19,
}).addTo(map);

L.control.zoom({ position: 'bottomright' }).addTo(map);

setTimeout(() => map.invalidateSize(), 100);

// ── State ──
let allCinemas     = [];
let markers        = {};
let activeId       = null;
let panelDate      = todayStr();
let highlightedIds = null;
let aaeOn          = false;
let voOn           = false;
let searchTimer    = null;

// ── Date ──
const dateInput = document.getElementById('date-filter');
dateInput.value = todayStr();

document.querySelectorAll('.dp-pill').forEach(btn => {
  btn.addEventListener('click', () => {
    const d = new Date();
    d.setDate(d.getDate() + +btn.dataset.offset);
    setDate(d.toISOString().slice(0, 10));
  });
});
dateInput.addEventListener('change', () => { if (dateInput.value) setDate(dateInput.value); });

function setDate(iso) {
  panelDate = iso;
  dateInput.value = iso;
  document.querySelectorAll('.dp-pill').forEach(btn => {
    const d = new Date();
    d.setDate(d.getDate() + +btn.dataset.offset);
    btn.classList.toggle('active', d.toISOString().slice(0, 10) === iso);
  });
  document.getElementById('panel-date-label').textContent = fmtDate(iso);
  if (activeId) loadShowtimes(activeId, iso);
}
setDate(todayStr());

// ── Art & Essai ──
document.getElementById('aae-btn').addEventListener('click', () => {
  aaeOn = !aaeOn;
  document.getElementById('aae-btn').classList.toggle('on', aaeOn);
  loadCinemas();
});

// ── VO ──
document.getElementById('vo-btn').addEventListener('click', () => {
  voOn = !voOn;
  document.getElementById('vo-btn').classList.toggle('on', voOn);
  loadCinemas();
  if (activeId) loadShowtimes(activeId, panelDate);
});

// ── Custom selects ──
let selectedRegion = '';
let selectedDept   = '';

function csOpen(btnId, listId) {
  const btn  = document.getElementById(btnId);
  const list = document.getElementById(listId);
  const isOpen = list.classList.contains('open');
  csCloseAll();
  if (!isOpen) { btn.classList.add('open'); list.classList.add('open'); }
}
function csCloseAll() {
  document.querySelectorAll('.cs-btn').forEach(b => b.classList.remove('open'));
  document.querySelectorAll('.cs-list').forEach(l => l.classList.remove('open'));
}
document.addEventListener('click', e => {
  if (!e.target.closest('.cs-wrap')) csCloseAll();
});

document.getElementById('region-btn').addEventListener('click', () => csOpen('region-btn', 'region-list'));
document.getElementById('dept-btn').addEventListener('click',   () => csOpen('dept-btn',   'dept-list'));

function csSetRegion(value, label) {
  selectedRegion = value;
  selectedDept   = '';
  document.getElementById('region-val').textContent = label || 'Toute la France';
  document.getElementById('region-list').querySelectorAll('.cs-opt')
    .forEach(o => o.classList.toggle('selected', o.dataset.value === value));
  csCloseAll();
  onRegionChange();
}

function csSetDept(value, label) {
  selectedDept = value;
  document.getElementById('dept-val').textContent = label || 'Tous les départements';
  document.getElementById('dept-list').querySelectorAll('.cs-opt')
    .forEach(o => o.classList.toggle('selected', o.dataset.value === value));
  csCloseAll();
  loadCinemas();
}

// ── Cinemas ──
async function loadCinemas() {
  const region = selectedRegion;
  const dept   = selectedDept;
  const p = new URLSearchParams();
  if (dept)        p.set('dept', dept);
  else if (region) p.set('region', region);
  if (aaeOn) p.set('art_et_essai', 'true');
  if (voOn)  { p.set('vo', 'true'); p.set('show_date', panelDate); }
  const qs = p.toString() ? '?' + p : '';
  try {
    allCinemas = await fetch('/api/cinemas' + qs).then(r => r.json());
  } catch (e) {
    console.error('loadCinemas:', e);
    return;
  }
  renderMarkers(allCinemas);
  document.getElementById('cinema-count').textContent = allCinemas.length;
}

function mkStyle(cinema, active) {
  const dim = highlightedIds !== null && !highlightedIds.has(cinema.id);
  return {
    renderer:    canvasRenderer,
    radius:      active ? 9 : 7,
    fillColor:   active ? '#ef4444' : (cinema.is_art_et_essai ? '#9b72f5' : '#f0b429'),
    color:       theme === 'dark' ? 'rgba(255,255,255,0.4)' : 'rgba(0,0,0,0.2)',
    weight:      active ? 2.5 : 1.5,
    fillOpacity: dim ? 0.12 : 0.9,
    opacity:     dim ? 0.18 : 1,
  };
}

function renderMarkers(cinemas) {
  Object.values(markers).forEach(m => map.removeLayer(m));
  markers = {};
  cinemas.forEach(c => {
    if (!c.lat || !c.lng) return;
    const m = L.circleMarker([c.lat, c.lng], mkStyle(c, false))
      .bindTooltip(c.name, { direction: 'top', offset: [0, -8] })
      .addTo(map);
    m.on('click', () => openPanel(c));
    markers[c.id] = m;
  });
}

function refreshMarkers() {
  allCinemas.forEach(c => markers[c.id]?.setStyle(mkStyle(c, c.id === activeId)));
}

function highlight(ids, movieTitle) {
  highlightedIds = ids ? new Set(ids) : null;
  refreshMarkers();
  const bar = document.getElementById('filter-bar');
  if (ids) {
    document.getElementById('fb-title').textContent = movieTitle || '';
    document.getElementById('fb-count').textContent =
      ids.length + ' cinéma' + (ids.length > 1 ? 's' : '');
    bar.classList.add('show');
  } else {
    bar.classList.remove('show');
  }
}

function clearHighlight() {
  highlight(null);
}

document.getElementById('fb-reset').addEventListener('click', clearHighlight);

// ── Panel ──
async function openPanel(cinema) {
  activeId = cinema.id;
  refreshMarkers();
  document.getElementById('close-panel').style.display = '';
  document.getElementById('panel-name').textContent = cinema.name;
  document.getElementById('panel-date-label').textContent = fmtDate(panelDate);
  document.getElementById('panel-meta').innerHTML = [
    cinema.city            && `<span class="tag">${esc(cinema.city)}</span>`,
    cinema.department      && `<span class="tag">${esc(cinema.department)}</span>`,
    cinema.screens         && `<span class="tag">${cinema.screens} salle${cinema.screens > 1 ? 's' : ''}</span>`,
    cinema.seats           && `<span class="tag">${cinema.seats} fauteuils</span>`,
    cinema.is_art_et_essai && `<span class="tag aae">Art &amp; Essai</span>`,
  ].filter(Boolean).join('');
  document.getElementById('panel').classList.add('open');
  showPanelBody();
  document.getElementById('panel-body').innerHTML =
    '<div class="empty-msg"><span class="icon">⏳</span>Chargement…</div>';
  await loadShowtimes(cinema.id, panelDate);
}

function closePanel() {
  activeId = null;
  refreshMarkers();
  showWelcome();
  document.getElementById('close-panel').style.display = 'none';
  document.getElementById('panel-name').textContent = fmtDate(panelDate);
  document.getElementById('panel-meta').innerHTML = '';
}

async function loadShowtimes(id, date) {
  let data;
  try { data = await fetch(`/api/cinemas/${id}/showtimes?show_date=${date}`).then(r => r.json()); }
  catch (e) { data = null; }
  const body = document.getElementById('panel-body');
  if (!data || !data.movies.length) {
    body.innerHTML = '<div class="empty-msg"><span class="icon">🎞️</span>Aucune séance pour cette date.</div>';
    return;
  }
  const movies = voOn
    ? data.movies
        .map(m => ({ ...m, showtimes: m.showtimes.filter(s => s.version === 'VO') }))
        .filter(m => m.showtimes.length > 0)
    : data.movies;
  if (!movies.length) {
    body.innerHTML = '<div class="empty-msg"><span class="icon">🎞️</span>Aucune séance VO pour cette date.</div>';
    return;
  }
  body.innerHTML = movies.map(m => {
    const img = m.poster_url
      ? `<img class="poster" src="${m.poster_url}" alt="" loading="lazy" onerror="this.outerHTML='<div class=\\'poster-ph\\'>🎬</div>'">`
      : `<div class="poster-ph">🎬</div>`;
    const chips = m.showtimes.map(s => {
      const v = s.version || 'VF';
      return `<span class="chip">${s.starts_at.slice(11,16)} <span class="ver ${v==='VO'?'vo':'vf'}">${v}</span></span>`;
    }).join('');
    return `<div class="movie-card">${img}<div class="movie-info"><div class="movie-title">${esc(m.title)}</div><div class="times">${chips}</div></div></div>`;
  }).join('');
}

document.getElementById('prev-day').addEventListener('click', () => {
  const d = new Date(panelDate + 'T12:00:00'); d.setDate(d.getDate() - 1);
  setDate(d.toISOString().slice(0, 10));
});
document.getElementById('next-day').addEventListener('click', () => {
  const d = new Date(panelDate + 'T12:00:00'); d.setDate(d.getDate() + 1);
  setDate(d.toISOString().slice(0, 10));
});
document.getElementById('close-panel').addEventListener('click', closePanel);

// ── Search ──
const searchInput = document.getElementById('search-input');
const searchDD    = document.getElementById('search-dropdown');

searchInput.addEventListener('input', () => {
  clearTimeout(searchTimer);
  const q = searchInput.value.trim();
  if (q.length < 2) { closeDD(); return; }
  searchTimer = setTimeout(() => doSearch(q), 240);
});
searchInput.addEventListener('keydown', e => {
  if (e.key === 'Escape') { closeDD(); searchInput.blur(); }
});
document.addEventListener('click', e => {
  if (!searchInput.contains(e.target) && !searchDD.contains(e.target)) closeDD();
});

async function doSearch(q) {
  let data;
  try { data = await fetch(`/api/search?q=${encodeURIComponent(q)}&show_date=${panelDate}`).then(r => r.json()); }
  catch(e) { return; }
  const { cinemas = [], movies = [] } = data;
  if (!cinemas.length && !movies.length) {
    searchDD.innerHTML = '<div class="dd-empty">Aucun résultat</div>';
    searchDD.classList.add('show'); return;
  }
  let html = '';
  if (cinemas.length) {
    html += `<div class="dd-section"><div class="dd-label">Cinémas</div>${
      cinemas.map(c => `<div class="dd-item" data-action="cinema" data-id="${c.id}" data-lat="${c.lat}" data-lng="${c.lng}">
        <div class="dd-dot" style="background:${c.is_art_et_essai?'#9b72f5':'#f0b429'}"></div>
        <div><div class="dd-main">${esc(c.name)}</div><div class="dd-sub">${[c.city,c.department].filter(Boolean).join(' · ')}</div></div>
      </div>`).join('')}</div>`;
  }
  if (movies.length) {
    html += `<div class="dd-section"><div class="dd-label">Films à l'affiche</div>${
      movies.map(m => `<div class="dd-item" data-action="movie" data-ids="${m.cinemas.map(c=>c.id).join(',')}">
        ${m.poster_url ? `<img class="dd-poster" src="${m.poster_url}" alt="" loading="lazy">` : `<div class="dd-poster-ph">🎬</div>`}
        <div><div class="dd-main">${esc(m.title)}</div><div class="dd-sub">${m.cinemas.length} cinéma${m.cinemas.length>1?'s':''}</div></div>
      </div>`).join('')}</div>`;
  }
  searchDD.innerHTML = html;
  searchDD.classList.add('show');
  searchDD.querySelectorAll('[data-action=cinema]').forEach(el => {
    el.addEventListener('click', () => {
      map.setView([+el.dataset.lat, +el.dataset.lng], 15, { animate: true, duration: 0.5 });
      clearHighlight();
      openPanel(allCinemas.find(c => c.id === +el.dataset.id));
      closeDD(); searchInput.value = '';
    });
  });
  searchDD.querySelectorAll('[data-action=movie]').forEach(el => {
    el.addEventListener('click', () => {
      const ids = el.dataset.ids.split(',').map(Number);
      const title = el.querySelector('.dd-main').textContent;
      highlight(ids, title);
      const pts = allCinemas.filter(c => ids.includes(c.id) && c.lat && c.lng).map(c => [c.lat, c.lng]);
      if (pts.length) map.fitBounds(pts, { padding: [60,60], maxZoom: 14, animate: true });
      closeDD(); searchInput.value = '';
    });
  });
}
function closeDD() { searchDD.classList.remove('show'); searchDD.innerHTML = ''; }

// ── Régions / Départements ──
const REGIONS = [
  { name: 'Île-de-France',            depts: ['Essonne','Hauts-de-Seine','Paris','Seine-et-Marne','Seine-Saint-Denis','Val-de-Marne','Val-d\'Oise','Yvelines'] },
  { name: 'Auvergne-Rhône-Alpes',     depts: ['Ain','Allier','Ardèche','Cantal','Drôme','Haute-Loire','Haute-Savoie','Isère','Loire','Puy-de-Dôme','Rhône','Savoie'] },
  { name: 'Bourgogne-Franche-Comté',  depts: ['Côte-d\'Or','Doubs','Jura','Nièvre','Haute-Saône','Saône-et-Loire','Territoire de Belfort','Yonne'] },
  { name: 'Bretagne',                 depts: ['Côtes-d\'Armor','Finistère','Ille-et-Vilaine','Morbihan'] },
  { name: 'Centre-Val de Loire',      depts: ['Cher','Eure-et-Loir','Indre','Indre-et-Loire','Loir-et-Cher','Loiret'] },
  { name: 'Corse',                    depts: ['Corse-du-Sud','Haute-Corse'] },
  { name: 'Grand Est',                depts: ['Ardennes','Aube','Bas-Rhin','Haut-Rhin','Haute-Marne','Marne','Meurthe-et-Moselle','Meuse','Moselle','Vosges'] },
  { name: 'Hauts-de-France',          depts: ['Aisne','Nord','Oise','Pas-de-Calais','Somme'] },
  { name: 'Normandie',                depts: ['Calvados','Eure','Manche','Orne','Seine-Maritime'] },
  { name: 'Nouvelle-Aquitaine',       depts: ['Charente','Charente-Maritime','Corrèze','Creuse','Deux-Sèvres','Dordogne','Gironde','Landes','Lot-et-Garonne','Pyrénées-Atlantiques','Vienne','Haute-Vienne'] },
  { name: 'Occitanie',                depts: ['Ariège','Aude','Aveyron','Gard','Gers','Haute-Garonne','Hautes-Pyrénées','Hérault','Lot','Lozère','Pyrénées-Orientales','Tarn','Tarn-et-Garonne'] },
  { name: 'Pays de la Loire',         depts: ['Loire-Atlantique','Maine-et-Loire','Mayenne','Sarthe','Vendée'] },
  { name: 'Provence-Alpes-Côte d\'Azur', depts: ['Alpes-de-Haute-Provence','Alpes-Maritimes','Bouches-du-Rhône','Hautes-Alpes','Var','Vaucluse'] },
  { name: 'Outre-mer',                depts: ['Guadeloupe','Guyane','La Réunion','Martinique','Mayotte'] },
];

function buildRegionSelect() {
  const list = document.getElementById('region-list');
  const reset = document.createElement('div');
  reset.className = 'cs-opt selected';
  reset.dataset.value = '';
  reset.textContent = 'Toute la France';
  reset.addEventListener('click', () => csSetRegion('', 'Toute la France'));
  list.appendChild(reset);

  REGIONS.forEach(r => {
    const o = document.createElement('div');
    o.className = 'cs-opt';
    o.dataset.value = r.name;
    o.textContent = r.name;
    o.addEventListener('click', () => csSetRegion(r.name, r.name));
    list.appendChild(o);
  });
}

function onRegionChange() {
  const deptWrap = document.getElementById('dept-wrap');
  const deptList = document.getElementById('dept-list');

  deptList.innerHTML = '';
  const reset = document.createElement('div');
  reset.className = 'cs-opt selected';
  reset.dataset.value = '';
  reset.textContent = 'Tous les départements';
  reset.addEventListener('click', () => csSetDept('', 'Tous les départements'));
  deptList.appendChild(reset);

  if (selectedRegion) {
    const r = REGIONS.find(r => r.name === selectedRegion);
    if (r) {
      r.depts.forEach(dept => {
        const o = document.createElement('div');
        o.className = 'cs-opt';
        o.dataset.value = dept;
        o.textContent = dept;
        o.addEventListener('click', () => csSetDept(dept, dept));
        deptList.appendChild(o);
      });
    }
    deptWrap.style.display = '';
  } else {
    deptWrap.style.display = 'none';
  }

  loadCinemas();
}

// ── Utils ──
function todayStr() { return new Date().toISOString().slice(0, 10); }
function fmtDate(iso) { return new Date(iso + 'T12:00:00').toLocaleDateString('fr-FR', { weekday: 'long', day: 'numeric', month: 'long' }); }
function esc(s) { return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;'); }

// ── Accueil ──
async function loadWelcome() {
  const welcome = document.getElementById('welcome');
  try {
    const d = await fetch(`/api/today?show_date=${panelDate}`).then(r => r.json());
    const s = d.stats;
    welcome.innerHTML = `
      <div class="w-stats">
        <div class="w-stat">
          <div class="val">${s.cinemas_with_showtimes}</div>
          <div class="lbl">cinémas programmés</div>
        </div>
        <div class="w-stat">
          <div class="val">${s.movie_count}</div>
          <div class="lbl">films à l'affiche</div>
        </div>
      </div>
      <div class="w-section-title">Top films du jour</div>
      <div id="w-movies">
        ${d.top_movies.map((m, i) => {
          const ids = m.cinema_ids.split(',');
          return `<div class="w-movie" data-ids="${ids.join(',')}" data-title="${esc(m.movie_title)}">
            ${m.poster_url
              ? `<img class="w-poster" src="${m.poster_url}" alt="" loading="lazy" onerror="this.outerHTML='<div class=\\'w-poster-ph\\'>🎬</div>'">`
              : `<div class="w-poster-ph">🎬</div>`}
            <div class="w-movie-info">
              <div class="w-movie-title">${esc(m.movie_title)}</div>
              <div class="w-movie-sub">${m.cinema_count} cinéma${m.cinema_count > 1 ? 's' : ''}</div>
            </div>
            <div class="w-rank">#${i + 1}</div>
          </div>`;
        }).join('')}
      </div>`;

    welcome.querySelectorAll('.w-movie').forEach(el => {
      el.addEventListener('click', () => {
        const ids = el.dataset.ids.split(',').map(Number);
        const title = el.dataset.title;
        highlight(ids, title);
        const pts = allCinemas.filter(c => ids.includes(c.id) && c.lat && c.lng).map(c => [c.lat, c.lng]);
        if (pts.length) map.fitBounds(pts, { padding: [60, 60], maxZoom: 14, animate: true });
      });
    });
  } catch(e) {
    welcome.innerHTML = '<div style="padding:20px;color:var(--muted);font-size:.82rem;text-align:center">Clique sur un cinéma<br>pour voir ses séances</div>';
  }
}

function showWelcome() {
  document.getElementById('welcome').style.display = '';
  document.getElementById('panel-body').style.display = 'none';
}

function showPanelBody() {
  document.getElementById('welcome').style.display = 'none';
  document.getElementById('panel-body').style.display = '';
}

// ── Boot ──
buildRegionSelect();
loadCinemas();
setTheme(theme);
document.getElementById('panel').classList.add('open');
document.getElementById('close-panel').style.display = 'none';
document.getElementById('panel-name').textContent = fmtDate(panelDate);
document.getElementById('panel-meta').innerHTML = '';
loadWelcome();
