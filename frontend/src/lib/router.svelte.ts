import {
  filtersQuery,
  parseFilters,
  parseRoute,
  routePath,
  type Route,
  type UrlFilters,
} from './routes';

/** État d'une entrée d'historique : `id` repère sa position de défilement, `prev` est
 * le chemin d'où l'on venait (un lien vers lui revient en arrière). */
type EntryState = { id: number; prev?: string };

const panel = () => document.querySelector<HTMLElement>('.panel');
const here = () => location.pathname + location.search;

/** Routeur minimal sur l'API History : la page et les filtres vivent dans l'URL. */
class Router {
  route = $state<Route>(parseRoute(location.pathname));
  filters = $state<UrlFilters>(parseFilters(location.search));
  /** Query brute, pour les paramètres propres à une page (`s` du programme partagé). */
  search = $state(location.search);

  /** Défilement (fenêtre, panneau) de chaque entrée, pour le retrouver au retour. Le
   * panneau défile seul sur grand écran : le navigateur ne sait pas le restaurer. */
  private positions = new Map<number, [number, number]>();
  private nextId = Date.now();
  private cancelRestore = () => {};

  constructor() {
    history.scrollRestoration = 'manual';
    if (typeof history.state?.id !== 'number') history.replaceState({ id: this.nextId++ }, '');
    addEventListener('popstate', () => {
      this.sync();
      this.restoreScroll(this.positions.get(this.state.id) ?? [0, 0]);
    });
    addEventListener(
      'scroll',
      () => this.positions.set(this.state.id, [scrollY, panel()?.scrollTop ?? 0]),
      { capture: true, passive: true },
    );
  }

  private get state(): EntryState {
    return history.state as EntryState;
  }

  /** La page revient sans ses données (rechargées) : on réessaie à chaque image jusqu'à
   * ce qu'elle soit assez haute, sans insister plus de 2 s ni contre le doigt. */
  private restoreScroll([top, panelTop]: [number, number]) {
    this.cancelRestore();
    const deadline = performance.now() + 2000;
    let frame = 0;
    const step = () => {
      scrollTo({ top });
      panel()?.scrollTo({ top: panelTop });
      const done =
        Math.abs(scrollY - top) < 2 && Math.abs((panel()?.scrollTop ?? 0) - panelTop) < 2;
      if (!done && performance.now() < deadline) frame = requestAnimationFrame(step);
      else this.cancelRestore();
    };
    const stop = () => this.cancelRestore();
    this.cancelRestore = () => {
      cancelAnimationFrame(frame);
      removeEventListener('wheel', stop);
      removeEventListener('touchstart', stop);
      this.cancelRestore = () => {};
    };
    addEventListener('wheel', stop, { passive: true });
    addEventListener('touchstart', stop, { passive: true });
    frame = requestAnimationFrame(step);
  }

  private sync() {
    this.route = parseRoute(location.pathname);
    this.filters = parseFilters(location.search);
    this.search = location.search;
  }

  /** Navigation vers un chemin interne (`/film/42?date=…`), avec une entrée d'historique. */
  goto(path: string) {
    const from = here();
    if (path === from) return;
    // « À l'affiche » depuis une fiche ouverte de l'accueil : un vrai retour, qui
    // retrouve la liste là où on l'avait laissée.
    if (this.state.prev === path) {
      history.back();
      return;
    }
    this.cancelRestore();
    history.pushState({ id: this.nextId++, prev: from } satisfies EntryState, '', path);
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
    history.replaceState(history.state, '', location.pathname + filtersQuery(filters));
    this.filters = filters;
    this.search = location.search;
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
