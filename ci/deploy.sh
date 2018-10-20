#!/usr/bin/env sh

tar cvf "${PROJECT_NAME}-${TRAVIS_TAG}-${TARGET}.tar" "target/release/fse_dump"
