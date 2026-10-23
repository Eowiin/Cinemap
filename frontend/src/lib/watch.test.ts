import { describe, expect, it } from 'vitest';
import type { Watch } from './api/types';
import { watchRows } from './watch';

const p = (name: string) => ({ name, logo_url: null });
const watch = (parts: Partial<Watch>): Watch => ({
  link: null,
  free: [],
  flatrate: [],
  rent: [],
  buy: [],
  checked_at: '2026-10-20 03:00:00',
  ...parts,
});

describe('watchRows', () => {
  it('skips empty kinds and merges identical rent and buy', () => {
    const rows = watchRows(
      watch({ flatrate: [p('Netflix')], rent: [p('Apple TV Store')], buy: [p('Apple TV Store')] }),
    );
    expect(rows.map((r) => r.label)).toEqual(['Abonnement', 'Location ou achat']);
  });

  it('keeps rent and buy apart when they differ', () => {
    const rows = watchRows(watch({ rent: [p('Orange VOD')], buy: [p('Apple TV Store')] }));
    expect(rows.map((r) => r.label)).toEqual(['Location', 'Achat']);
  });

  it('is empty when nothing is offered', () => {
    expect(watchRows(watch({}))).toEqual([]);
  });
});
