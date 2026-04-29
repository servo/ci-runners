#!/usr/bin/env bash

set -eu

CONTAINER_CLI="${CONTAINER_CLI:-docker}"
COMMANDLINE_TOOLS_PATH="${COMMANDLINE_TOOLS_PATH:-https://repo.huaweicloud.com/harmonyos/ohpm/5.1.0/commandline-tools-linux-x64-5.1.0.840.zip}"
SKIP_HDC_KEY="${SKIP_HDC_KEY:-0}"
STAGED_COMMANDLINE_TOOLS_PATH=
STAGED_HDC_KEY_PATH=
STAGED_HDC_KEY_PUB_PATH=

cleanup() {
    if [[ -n "${STAGED_COMMANDLINE_TOOLS_PATH}" && -f "${STAGED_COMMANDLINE_TOOLS_PATH}" ]]
    then
        rm -f "${STAGED_COMMANDLINE_TOOLS_PATH}"
    fi

    if [[ -n "${STAGED_HDC_KEY_PATH}" && -f "${STAGED_HDC_KEY_PATH}" ]]
    then
        rm -f "${STAGED_HDC_KEY_PATH}"
    fi

    if [[ -n "${STAGED_HDC_KEY_PUB_PATH}" && -f "${STAGED_HDC_KEY_PUB_PATH}" ]]
    then
        rm -f "${STAGED_HDC_KEY_PUB_PATH}"
    fi
}

trap cleanup EXIT

if [[ -f "${COMMANDLINE_TOOLS_PATH}" ]]
then
    STAGED_COMMANDLINE_TOOLS_PATH="hos_commandline_tools/.commandline-tools.zip"
    cp "${COMMANDLINE_TOOLS_PATH}" "${STAGED_COMMANDLINE_TOOLS_PATH}"
    COMMANDLINE_TOOLS_PATH=".commandline-tools.zip"
fi

SERVO_GIT_HASH=$(git ls-remote https://github.com/servo/servo.git --branches refs/heads/main | awk '{ print $1}')
GITHUB_ACTIONS_RUNNER_VERSION="2.334.0"
MITMPROXY_VERSION="12.2.1"
RUST_VERSION="1.92.0"
UV_VERSION="0.9.28"
IMAGE_USERNAME=servo_ci


if [[ ! -f hos_builder/githubcli-archive-keyring.gpg ]]
then
    echo "Couldn't find github cli keyring. Downloading..."
    cd hos_builder && wget https://cli.github.com/packages/githubcli-archive-keyring.gpg && cd -
fi

STAGED_HDC_KEY_PATH="runner/.staged_hdckey"
STAGED_HDC_KEY_PUB_PATH="runner/.staged_hdckey.pub"

if [[ "${SKIP_HDC_KEY}" == "1" ]]
then
    : > "${STAGED_HDC_KEY_PATH}"
    : > "${STAGED_HDC_KEY_PUB_PATH}"
    echo "Skipping optional hdc key setup (SKIP_HDC_KEY=1)."
else
    if [[ ! -f runner/hdckey || ! -f runner/hdckey.pub ]]
    then
        echo "runner/hdckey and runner/hdckey.pub are required unless SKIP_HDC_KEY=1." >&2
        exit 1
    fi

    cp runner/hdckey "${STAGED_HDC_KEY_PATH}"
    cp runner/hdckey.pub "${STAGED_HDC_KEY_PUB_PATH}"
fi

# Build the helper images
"${CONTAINER_CLI}" build base -f base/Dockerfile -t "localhost/servo_gha_base:latest" --build-arg=USERNAME=${IMAGE_USERNAME}
"${CONTAINER_CLI}" build gh_runner -f gh_runner/Dockerfile -t "localhost/servo_gha_runner:${GITHUB_ACTIONS_RUNNER_VERSION}" \
    --build-arg=USERNAME=${IMAGE_USERNAME} \
    --build-arg=GITHUB_ACTIONS_RUNNER_VERSION=${GITHUB_ACTIONS_RUNNER_VERSION}
"${CONTAINER_CLI}" build hos_commandline_tools -f hos_commandline_tools/Dockerfile -t "localhost/hos_commandline_tools:latest" \
   --build-arg=USERNAME=${IMAGE_USERNAME} \
   --build-arg=COMMANDLINE_TOOLS_PATH=${COMMANDLINE_TOOLS_PATH}

# Build the actual images

"${CONTAINER_CLI}" build hos_builder -f hos_builder/Dockerfile -t localhost/servo_gha_hos_builder:latest \
      --build-arg SERVO_GIT_HASH=${SERVO_GIT_HASH} \
      --build-arg GITHUB_ACTIONS_RUNNER_VERSION=${GITHUB_ACTIONS_RUNNER_VERSION} \
      --build-arg RUST_VERSION=${RUST_VERSION} \
      --build-arg UV_VERSION=${UV_VERSION} \
      --build-arg USERNAME=${IMAGE_USERNAME}

"${CONTAINER_CLI}" build runner -f runner/Dockerfile -t localhost/servo_gha_hos_runner:latest \
    --build-arg GITHUB_ACTIONS_RUNNER_VERSION=${GITHUB_ACTIONS_RUNNER_VERSION} \
    --build-arg MITMPROXY_VERSION=${MITMPROXY_VERSION} \
    --build-arg UV_VERSION=${UV_VERSION} \
    --build-arg SKIP_HDC_KEY=${SKIP_HDC_KEY} \
    --build-arg USERNAME=${IMAGE_USERNAME}
