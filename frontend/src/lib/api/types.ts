// Types du contrat docs/API.md (qui fait foi). Toute modification commence là-bas.

export type Version = 'VF' | 'VO' | 'VOST';
/** Paramètre `version` : `VO` renvoie VO + VOST. */
export type VersionFilter = 'VF' | 'VO';

export type CinemaSummary = {
  id: string;
  name: string;
  city: string | null;
  lat: number;
  lng: number;
  art_et_essai: boolean;
  distance_km: number | null;
};

export type Cinema = CinemaSummary & {
  address: string | null;
  postal_code: string | null;
  department: string | null;
  screens: number | null;
  seats: number | null;
  allocine_url: string;
};

export type MovieSummary = {
  id: number;
  title: string;
  poster_url: string | null;
  genres: string[];
  runtime_min: number | null;
  release_date: string | null;
};

export type Person = { name: string; role: string | null };

export type Movie = MovieSummary & {
  original_title: string | null;
  synopsis: string | null;
  directors: string[];
  cast: Person[];
  countries: string[];
  production_year: number | null;
  certificate: string | null;
  backdrop_url: string | null;
  trailer_url: string | null;
  rating: number | null;
  user_rating: number | null;
};

export type Showtime = {
  id: string;
  starts_at: string;
  version: Version;
  formats: string[];
  booking_url: string | null;
};

export type Meta = {
  today: string;
  last_scrape_at: string | null;
  dates_available: string[];
  cinema_count: number;
  movie_count: number;
};

export type CinemaShowtimes = {
  cinema: Cinema;
  date: string;
  dates: string[];
  movies: { movie: MovieSummary; showtimes: Showtime[] }[];
};

export type NowShowing = {
  date: string;
  movies: {
    movie: MovieSummary;
    cinema_count: number;
    showtime_count: number;
    next_showtime: string;
  }[];
};

export type MovieShowtimes = {
  movie: MovieSummary;
  date: string;
  dates: string[];
  cinemas: { cinema: CinemaSummary; showtimes: Showtime[] }[];
};

export type SearchResults = {
  movies: MovieSummary[];
  cinemas: CinemaSummary[];
};
