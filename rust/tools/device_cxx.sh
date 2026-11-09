#!/bin/sh
# One Eigen translation unit needs more than a gigabyte, so C++ compiles run one at a time.
exec flock "${OPENPILOT_BUILD_STORE:-/data/openpilot-build}/cxx.lock" /usr/bin/g++ "$@"
