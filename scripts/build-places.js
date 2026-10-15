// Fabrique la base des lieux intégrée à PicLib (src-tauri/data/places/)
// à partir des fichiers GeoNames « cities500.txt » et « admin1CodesASCII.txt »
// (licence CC BY 4.0). Appelé par scripts/build-places.sh, qui les télécharge.
//
// Fichiers produits, en texte UTF-8 séparé par des tabulations :
// - countries.tsv : code ISO du pays, nom français, nom anglais ;
// - regions.tsv : code du pays, code de la région, nom de la région ;
// - places.tsv : lieux regroupés par région. Une ligne « >FR<tab>11 » ouvre
//   le groupe d'une région (code vide : région inconnue), puis une ligne par
//   lieu : latitude, longitude, nom.
//
// Les noms de pays viennent des données Unicode (CLDR) intégrées à Node,
// dans les deux langues de PicLib.

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";

/**
 * Lieux habités exclus : quartiers d'une ville (PPLX), lieux abandonnés,
 * historiques ou détruits. Une photo prise dans Paris doit donner « Paris »,
 * pas le nom d'un quartier.
 */
const EXCLUDED_FEATURES = new Set(["PPLX", "PPLQ", "PPLH", "PPLW", "PPLCH"]);

/** Précision des coordonnées : 4 décimales, soit une dizaine de mètres. */
const DECIMALS = 4;

const [sourceDir, outputDir] = process.argv.slice(2);
if (sourceDir === undefined || outputDir === undefined) {
  process.stderr.write(
    "usage : node scripts/build-places.js <dossier GeoNames> <dossier de sortie>\n",
  );
  process.exit(1);
}

/** Lignes non vides d'un fichier texte de GeoNames, découpées en colonnes. */
function rows(fileName) {
  return readFileSync(join(sourceDir, fileName), "utf8")
    .split("\n")
    .filter((line) => line !== "" && !line.startsWith("#"))
    .map((line) => line.split("\t"));
}

/** Texte sans tabulation ni retour à la ligne, qui casseraient le format. */
function clean(text) {
  return text.replace(/[\t\r\n]+/g, " ").trim();
}

function coordinate(text) {
  return String(Number(Number(text).toFixed(DECIMALS)));
}

const compare = (a, b) => (a < b ? -1 : a > b ? 1 : 0);

// Colonnes de cities500 : 1 nom, 4 latitude, 5 longitude, 7 type de lieu,
// 8 code du pays, 10 code de la région (admin1).
const places = rows("cities500.txt")
  .filter((columns) => !EXCLUDED_FEATURES.has(columns[7]))
  .map((columns) => ({
    name: clean(columns[1]),
    latitude: coordinate(columns[4]),
    longitude: coordinate(columns[5]),
    country: columns[8],
    region: columns[10] === "00" ? "" : columns[10],
  }))
  .filter((place) => place.name !== "" && /^[A-Z]{2}$/.test(place.country))
  .sort(
    (a, b) =>
      compare(a.country, b.country) ||
      compare(a.region, b.region) ||
      compare(a.name, b.name) ||
      compare(a.latitude, b.latitude) ||
      compare(a.longitude, b.longitude),
  );

const countryCodes = [...new Set(places.map((place) => place.country))].sort();
const frenchNames = new Intl.DisplayNames(["fr"], { type: "region" });
const englishNames = new Intl.DisplayNames(["en"], { type: "region" });
const countries = countryCodes.map((code) =>
  [code, frenchNames.of(code) ?? code, englishNames.of(code) ?? code].join(
    "\t",
  ),
);

// Seules les régions qui contiennent au moins un lieu sont gardées.
const usedRegions = new Set(
  places.map((place) => `${place.country}.${place.region}`),
);
const regions = rows("admin1CodesASCII.txt")
  .filter((columns) => usedRegions.has(columns[0]))
  .map((columns) => {
    const [country, region] = columns[0].split(".");
    return [country, region, clean(columns[1])];
  })
  .filter((columns) => columns[2] !== "")
  .sort((a, b) => compare(a[0], b[0]) || compare(a[1], b[1]))
  .map((columns) => columns.join("\t"));

const placeLines = [];
let group = null;
for (const place of places) {
  const key = `${place.country}\t${place.region}`;
  if (key !== group) {
    placeLines.push(`>${key}`);
    group = key;
  }
  placeLines.push([place.latitude, place.longitude, place.name].join("\t"));
}

mkdirSync(outputDir, { recursive: true });
const write = (fileName, lines) => {
  writeFileSync(join(outputDir, fileName), `${lines.join("\n")}\n`);
};
write("countries.tsv", countries);
write("regions.tsv", regions);
write("places.tsv", placeLines);
process.stdout.write(
  `${String(places.length)} lieux, ${String(regions.length)} régions, ${String(countries.length)} pays\n`,
);
