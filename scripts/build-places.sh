#!/usr/bin/env bash
# Télécharge les lieux habités de plus de 500 habitants de GeoNames
# (https://www.geonames.org, licence CC BY 4.0) et en fabrique la base des
# lieux intégrée à PicLib (src-tauri/data/places/, versionnée dans Git).
# À relancer seulement pour mettre la base à jour.
set -euo pipefail

cd "$(dirname "$0")/.."
source_url="https://download.geonames.org/export/dump"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

curl --fail --silent --show-error --location \
  --output "$work_dir/cities500.zip" "$source_url/cities500.zip"
curl --fail --silent --show-error --location \
  --output "$work_dir/admin1CodesASCII.txt" "$source_url/admin1CodesASCII.txt"
unzip -q "$work_dir/cities500.zip" -d "$work_dir"

node scripts/build-places.js "$work_dir" src-tauri/data/places
