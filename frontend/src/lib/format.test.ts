import { describe, expect, it } from 'vitest';
import {
  addDays,
  dateLabel,
  effectiveAfter,
  floorQuarter,
  formatDistance,
  formatRuntime,
  isAfterMidnight,
  parisNow,
  posterUrl,
  timeAgo,
  timeOf,
} from './format';

describe('parisNow', () => {
  it('uses Paris time, not UTC', () => {
    // 23:30 UTC le 8 octobre = 01:30 le 9 à Paris (heure d'été).
    expect(parisNow(new Date('2026-10-08T23:30:00Z'))).toEqual({
      date: '2026-10-09',
      time: '01:30',
    });
    // Hiver : UTC+1.
    expect(parisNow(new Date('2026-12-31T23:10:00Z'))).toEqual({
      date: '2027-01-01',
      time: '00:10',
    });
  });
});

describe('dates', () => {
  it('adds days across months and DST', () => {
    expect(addDays('2026-10-31', 1)).toBe('2026-11-01');
    expect(addDays('2026-10-24', 2)).toBe('2026-10-26');
  });

  it('labels today, tomorrow and later days', () => {
    expect(dateLabel('2026-10-08', '2026-10-08')).toBe("Aujourd'hui");
    expect(dateLabel('2026-10-09', '2026-10-08')).toBe('Demain');
    expect(dateLabel('2026-10-14', '2026-10-08')).toBe('mer. 14 oct.');
  });
});

describe('showtimes', () => {
  it('reads the time and spots after-midnight showtimes', () => {
    expect(timeOf('2026-10-08T20:30:00')).toBe('20:30');
    expect(isAfterMidnight('2026-10-09T00:15:00', '2026-10-08')).toBe(true);
    expect(isAfterMidnight('2026-10-08T23:59:00', '2026-10-08')).toBe(false);
  });

  it('never asks for past showtimes today', () => {
    expect(effectiveAfter('2026-10-08', '2026-10-08', '14:10', null)).toBe('14:10');
    expect(effectiveAfter('2026-10-08', '2026-10-08', '14:10', '20:00')).toBe('20:00');
    expect(effectiveAfter('2026-10-08', '2026-10-08', '21:00', '20:00')).toBe('21:00');
    expect(effectiveAfter('2026-10-09', '2026-10-08', '21:00', null)).toBeNull();
    expect(effectiveAfter('2026-10-09', '2026-10-08', '21:00', '20:00')).toBe('20:00');
  });
});

describe('floorQuarter', () => {
  it('rounds down to the quarter hour', () => {
    expect(floorQuarter('20:14')).toBe('20:00');
    expect(floorQuarter('20:15')).toBe('20:15');
    expect(floorQuarter('09:59')).toBe('09:45');
  });
});

describe('formatting', () => {
  it('formats runtimes', () => {
    expect(formatRuntime(125)).toBe('2 h 05');
    expect(formatRuntime(45)).toBe('45 min');
    expect(formatRuntime(null)).toBeNull();
  });

  it('formats distances', () => {
    expect(formatDistance(0.3)).toBe('300 m');
    expect(formatDistance(1.2)).toBe('1,2 km');
    expect(formatDistance(12)).toBe('12 km');
    expect(formatDistance(null)).toBeNull();
  });

  it('formats elapsed time', () => {
    const now = new Date('2026-10-08T12:00:00Z');
    expect(timeAgo('2026-10-08T11:30:00Z', now)).toBe('il y a 30 min');
    expect(timeAgo('2026-10-08T04:00:00Z', now)).toBe('il y a 8 h');
  });

  it('resizes AlloCiné posters only', () => {
    expect(posterUrl('https://fr.web.img5.acsta.net/pictures/17/1.jpg', 160, 213)).toBe(
      'https://fr.web.img5.acsta.net/c_160_213/pictures/17/1.jpg',
    );
    expect(posterUrl('https://example.com/a.jpg', 160, 213)).toBe('https://example.com/a.jpg');
    expect(posterUrl(null, 160, 213)).toBeNull();
  });
});
