#!/usr/bin/env sh

zip "${PROJECT_NAME}-${TRAVIS_TAG}-${TARGET}".zip "target/$TARGET/fse_dump"
