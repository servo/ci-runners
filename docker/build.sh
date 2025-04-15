#!/usr/bin/env bash

set -eu

SERVO_GIT_HASH=$(git ls-remote https://github.com/servo/servo.git --branches refs/heads/main | awk '{ print $1}')
GITHUB_ACTIONS_RUNNER_VERSION="2.323.0"
RUST_VERSION="1.85.0"
UV_VERSION="0.6.14"
IMAGE_USERNAME=servo_ci


if [[ ! -f hos_builder/githubcli-archive-keyring.gpg ]]
then
    echo "Couldn't find github cli keyring. Downloading..."
    cd hos_builder && wget https://cli.github.com/packages/githubcli-archive-keyring.gpg && cd -
fi

# Build the helper images
docker build base -f base/Dockerfile -t servo_gha_base:latest --build-arg=USERNAME=${IMAGE_USERNAME}
docker build gh_runner -f gh_runner/Dockerfile -t "servo_gha_runner:${GITHUB_ACTIONS_RUNNER_VERSION}" \
    --build-arg=USERNAME=${IMAGE_USERNAME} \
    --build-arg=GITHUB_ACTIONS_RUNNER_VERSION=${GITHUB_ACTIONS_RUNNER_VERSION}
docker build hos_commandline_tools -f hos_commandline_tools/Dockerfile -t "hos_commandline_tools" \
   --build-arg=USERNAME=${IMAGE_USERNAME}

# Build the actual images

docker build hos_builder -f hos_builder/Dockerfile -t servo_gha_hos_builder:latest \
     --build-arg SERVO_GIT_HASH=${SERVO_GIT_HASH} \
     --build-arg HOS_COMMANDLINE_TOOLS_VERSION=${HOS_COMMANDLINE_TOOLS_VERSION} \
     --build-arg GITHUB_ACTIONS_RUNNER_VERSION=${GITHUB_ACTIONS_RUNNER_VERSION} \
     --build-arg RUST_VERSION=${RUST_VERSION} \
     --build-arg UV_VERSION=${UV_VERSION} \
     --build-arg USERNAME=${IMAGE_USERNAME}

docker build runner -f runner/Dockerfile -t servo_gha_hos_runner:latest \
    --build-arg HOS_COMMANDLINE_TOOLS_VERSION=${HOS_COMMANDLINE_TOOLS_VERSION} \
    --build-arg GITHUB_ACTIONS_RUNNER_VERSION=${GITHUB_ACTIONS_RUNNER_VERSION} \
    --build-arg USERNAME=${IMAGE_USERNAME}
