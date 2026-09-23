#!/bin/sh
# Fetches the site's contract with the apps (frontend/shared/appContract.json
# in oeee-cafe/web) into src/testdata/, which src/contract.rs tests against.
# Run it after the site changes what it says to the apps, then `cargo test`:
# what fails is what this app has to follow.
#
# The site's checkout is taken to sit beside this one; OEEE_CAFE_WEB points
# anywhere else, a worktree of it included.
set -eu
here=$(git rev-parse --show-toplevel)
web=${OEEE_CAFE_WEB:-$here/../oeee-cafe-web}
cp "$web/frontend/shared/appContract.json" "$here/src/testdata/appContract.json"
echo "src/testdata/appContract.json from $web"
