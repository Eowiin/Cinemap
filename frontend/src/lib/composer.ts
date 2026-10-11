// Programme automatique : caser 2 à 4 films dans la journée, autour des séances déjà
// prévues. Fonctions pures, testées ; tout est calculé dans le navigateur.

import {
  endMinutes,
  MARGIN_MIN,
  startMinutes,
  travel,
  type PlanStop,
  type Travel,
} from './planner';

/** Pénalité d'un enchaînement « juste » (< 10 min de battement), en minutes d'attente. */
export const TIGHT_PENALTY_MIN = 15;
/** Journées proposées. */
export const MAX_PLANS = 3;
/**
 * Séances essayées par film à chaque étape, dans l'ordre de début : une séance plus
 * tardive n'ajoute que de l'attente. Borne le calcul : 4 films à 50 séances chacun
 * font au plus 24 ordres × 50 × 6 × 6 × 6 ≈ 260 000 essais.
 */
const PER_STEP = 6;

export type ComposedPlan = {
  /** Toute la journée (séances déjà prévues comprises), dans l'ordre. */
  stops: PlanStop[];
  /** Les séances ajoutées par la proposition. */
  added: PlanStop[];
  /** Temps entre les films, trajets compris (minutes). */
  wait: number;
  travels: number;
  tight: number;
  /** Fin de la dernière séance (minutes depuis minuit du jour ciné). */
  end: number;
};

const score = (p: Pick<ComposedPlan, 'wait' | 'tight'>) => p.wait + p.tight * TIGHT_PENALTY_MIN;

/** Le moins d'attente (« juste » pénalisé), puis le moins de trajets, puis finir tôt. */
export function comparePlans(
  a: Pick<ComposedPlan, 'wait' | 'tight' | 'travels' | 'end'>,
  b: Pick<ComposedPlan, 'wait' | 'tight' | 'travels' | 'end'>,
): number {
  return score(a) - score(b) || a.travels - b.travels || a.end - b.end;
}

// Le calcul essaie des centaines de milliers d'enchaînements : heures et trajets sont
// calculés une fois (`Node`, cache des trajets entre cinémas), et une journée n'est
// construite que si elle entre dans le classement.

type Node = { stop: PlanStop; start: number; end: number };

const toNode = (stop: PlanStop): Node => ({
  stop,
  start: startMinutes(stop),
  end: endMinutes(stop),
});

function travelCache() {
  const cache = new Map<string, Travel>();
  return (a: PlanStop, b: PlanStop): Travel => {
    const key = `${a.cinema.id}>${b.cinema.id}`;
    let t = cache.get(key);
    if (!t) {
      t = travel(a.cinema, b.cinema);
      cache.set(key, t);
    }
    return t;
  };
}

function permutations(n: number): number[][] {
  if (n === 0) return [[]];
  return permutations(n - 1).flatMap((p) =>
    Array.from({ length: n }, (_, i) => [...p.slice(0, i), n - 1, ...p.slice(i)]),
  );
}

/**
 * Les meilleures journées qui contiennent une séance de chaque film de `candidates`
 * (une liste de séances par film) en plus de `fixed`. Chaque ensemble n'est trouvé
 * qu'une fois : les films ajoutés sont pris dans l'ordre de leurs séances.
 */
export function compose(
  fixed: PlanStop[],
  candidates: PlanStop[][],
  max = MAX_PLANS,
): ComposedPlan[] {
  if (candidates.length === 0 || candidates.some((list) => list.length === 0)) return [];
  const lists = candidates.map((list) => list.map(toNode).sort((a, b) => a.start - b.start));
  const base = fixed.map(toNode).sort((a, b) => a.start - b.start);
  const travelOf = travelCache();

  /** 0 ok, 1 juste, 2 impossible (chevauchement ou trajet trop court). */
  const linkKind = (from: Node, to: Node) => {
    const gap = to.start - from.end;
    if (gap < 0) return 2;
    const slack = gap - travelOf(from.stop, to.stop).minutes;
    return slack < 0 ? 2 : slack < MARGIN_MIN ? 1 : 0;
  };

  const best: ComposedPlan[] = [];

  const record = (chain: Node[], added: Node[]) => {
    let wait = 0;
    let travels = 0;
    let tight = 0;
    for (let i = 0; i + 1 < chain.length; i++) {
      const from = chain[i]!;
      const to = chain[i + 1]!;
      wait += to.start - from.end;
      if (travelOf(from.stop, to.stop).mode !== 'same') travels++;
      if (linkKind(from, to) === 1) tight++;
    }
    const scored = { wait, travels, tight, end: chain[chain.length - 1]!.end };
    if (best.length === max && comparePlans(scored, best[max - 1]!) >= 0) return;
    best.push({ ...scored, stops: chain.map((n) => n.stop), added: added.map((n) => n.stop) });
    best.sort(comparePlans);
    if (best.length > max) best.pop();
  };

  /** Position d'insertion de `node` si elle ne crée ni chevauchement ni trajet trop court. */
  const slot = (chain: Node[], node: Node): number => {
    let i = 0;
    while (i < chain.length && chain[i]!.start <= node.start) i++;
    for (const n of chain) if (n.stop.movie.id === node.stop.movie.id) return -1;
    if (i > 0 && linkKind(chain[i - 1]!, node) === 2) return -1;
    if (i < chain.length && linkKind(node, chain[i]!) === 2) return -1;
    return i;
  };

  const dfs = (order: number[], k: number, chain: Node[], added: Node[], last: number) => {
    if (k === order.length) {
      record(chain, added);
      return;
    }
    let tried = 0;
    for (const node of lists[order[k]!]!) {
      if (node.start <= last) continue;
      const i = slot(chain, node);
      if (i === -1) continue;
      const next = [...chain.slice(0, i), node, ...chain.slice(i)];
      dfs(order, k + 1, next, [...added, node], node.start);
      if (++tried >= PER_STEP) break;
    }
  };

  for (const order of permutations(lists.length)) dfs(order, 0, base, [], -Infinity);
  return best;
}
/**
 * `compose`, et si les films ne tiennent pas tous dans la journée : les meilleures
 * journées sans l'un d'eux (`dropped` = son index), pour proposer quand même quelque chose.
 */
export function composeOrDrop(
  fixed: PlanStop[],
  candidates: PlanStop[][],
): { plans: ComposedPlan[]; dropped: number | null } {
  const plans = compose(fixed, candidates);
  if (plans.length || candidates.length < 2) return { plans, dropped: null };
  let result: { plans: ComposedPlan[]; dropped: number | null } = { plans: [], dropped: null };
  candidates.forEach((_, i) => {
    const without = compose(
      fixed,
      candidates.filter((__, j) => j !== i),
    );
    if (
      without.length &&
      (!result.plans.length || comparePlans(without[0]!, result.plans[0]!) < 0)
    ) {
      result = { plans: without, dropped: i };
    }
  });
  return result;
}
