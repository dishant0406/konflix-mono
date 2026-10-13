// Filter values the search page offers. These are UI values; the API maps them to
// AniList's enums, so changing a label here never changes the query.

export const types = [
  { value: "tv", label: "TV" },
  { value: "movie", label: "Movie" },
  { value: "ova", label: "OVA" },
  { value: "ona", label: "ONA" },
  { value: "special", label: "Special" },
  { value: "music", label: "Music" },
];

export const statuses = [
  { value: "currently-airing", label: "Currently Airing" },
  { value: "finished-airing", label: "Finished Airing" },
  { value: "not-yet-aired", label: "Not Yet Aired" },
];

export const seasons = [
  { value: "winter", label: "Winter" },
  { value: "spring", label: "Spring" },
  { value: "summer", label: "Summer" },
  { value: "fall", label: "Fall" },
];

export const sorts = [
  { value: "popularity", label: "Most Popular" },
  { value: "trending", label: "Trending" },
  { value: "score", label: "Highest Score" },
  { value: "favourites", label: "Most Favourited" },
  { value: "recent", label: "Newest" },
  { value: "title", label: "Title A-Z" },
];

// AniList's genre names, which are what the API filters on.
export const genres = [
  "Action", "Adventure", "Comedy", "Drama", "Ecchi", "Fantasy", "Horror",
  "Mahou Shoujo", "Mecha", "Music", "Mystery", "Psychological", "Romance",
  "Sci-Fi", "Slice of Life", "Sports", "Supernatural", "Thriller",
].map((value) => ({ value, label: value }));

export const currentYear = new Date().getFullYear();
export const years = Array.from({ length: 45 }, (_, index) => currentYear - index);
