#!/usr/bin/env sh

tar cvf "${PROJECT_NAME}-${TRAVIS_TAG}-${TARGET}.tar" "target/$TARGET/fse_dump"
