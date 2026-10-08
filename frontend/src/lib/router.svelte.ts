import {
  filtersQuery,
  parseFilters,
  parseRoute,
  routePath,
  type Route,
  type UrlFilters,
} from './routes';

/** Routeur minimal sur l'API History : la page et les filtres vivent dans l'URL. */
class Router {
  route = $state<Route>(parseRoute(location.pathname));
  filters = $state<UrlFilters>(parseFilters(location.search));

  constructor() {
    addEventListener('popstate', () => this.sync());
  }

  private sync() {
    this.route = parseRoute(location.pathname);
    this.filters = parseFilters(location.search);
  }

  /** Navigation vers un chemin interne (`/film/42?date=…`), avec une entrée d'historique. */
  goto(path: string) {
    if (path === location.pathname + location.search) return;
    history.pushState(null, '', path);
    this.sync();
    scrollTo({ top: 0 });
    document.querySelector('.panel')?.scrollTo({ top: 0 });
  }

  go(route: Route, filters: UrlFilters = this.filters) {
    this.goto(routePath(route) + filtersQuery(filters));
  }

  /** Change des filtres sans créer d'entrée d'historique (le bouton retour change de page, pas de filtre). */
  setFilters(changes: Partial<UrlFilters>) {
    const filters = { ...this.filters, ...changes };
    history.replaceState(null, '', location.pathname + filtersQuery(filters));
    this.filters = filters;
  }
}

export const router = new Router();

/** Intercepte les clics sur les liens internes pour éviter un rechargement de page. */
export function interceptLinks(event: MouseEvent) {
  if (event.defaultPrevented || event.button !== 0) return;
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
  const anchor = (event.target as Element | null)?.closest('a');
  if (!anchor || anchor.target || anchor.hasAttribute('download')) return;
  if (anchor.origin !== location.origin || anchor.pathname.startsWith('/api/')) return;
  event.preventDefault();
  router.goto(anchor.pathname + anchor.search);
}
