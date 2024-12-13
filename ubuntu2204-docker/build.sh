#!/usr/bin/env bash

set -eu

SERVO_GIT_HASH=$(git ls-remote https://github.com/servo/servo.git --branches refs/heads/main | awk '{ print $1}')

docker build . -f SignAndTest.Dockerfile -t servo_gha_hos_runner:latest
docker build . -t servo_gha_hos_builder:latest --build-arg=SERVO_GIT_HASH=${SERVO_GIT_HASH}
