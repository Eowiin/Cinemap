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
  zoomSnap:            0.25,
  zoomDelta:           1,
  wheelPxPerZoomLevel: 80,
  inertia:             true,
  inertiaDeceleration: 3000,
  inertiaMaxSpeed:     2000,
  easeLinearity:       0.2,
}).setView([48.856, 2.347], 11);

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

// ── Cinemas ──
async function loadCinemas() {
  const dept = document.getElementById('dept-filter').value;
  const p = new URLSearchParams();
  if (dept)  p.set('dept', dept);
  if (aaeOn) p.set('art_et_essai', 'true');
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
  body.innerHTML = data.movies.map(m => {
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
document.getElementById('dept-filter').addEventListener('change', loadCinemas);

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

// ── Départements ──
async function loadDepts() {
  try {
    const depts = await fetch('/api/departments').then(r => r.json());
    const sel = document.getElementById('dept-filter');
    depts.forEach(d => { const o = document.createElement('option'); o.value = d; o.textContent = d; sel.appendChild(o); });
  } catch(e) { console.error('loadDepts:', e); }
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
loadDepts();
loadCinemas();
setTheme(theme);
document.getElementById('panel').classList.add('open');
document.getElementById('close-panel').style.display = 'none';
document.getElementById('panel-name').textContent = fmtDate(panelDate);
document.getElementById('panel-meta').innerHTML = '';
loadWelcome();
