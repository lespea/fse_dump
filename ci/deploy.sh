#!/usr/bin/env sh

mkdir -p pfiles
if [[ "$TRAVIS_OS_NAME" == 'windows' ]]; then
    cp "target/release/${PROJECT_NAME}" "pfiles/${PROJECT_NAME}-${TRAVIS_TAG}-${TARGET}.exe"
else
    cp "target/release/${PROJECT_NAME}.exe" "pfiles/${PROJECT_NAME}-${TRAVIS_TAG}-${TARGET}"
fi
