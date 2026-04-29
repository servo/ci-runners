#!/usr/bin/env bash

set -eu

CONTAINER_CLI="${CONTAINER_CLI:-docker}"
COMMANDLINE_TOOLS_PATH="https://repo.huaweicloud.com/harmonyos/ohpm/5.1.0/commandline-tools-linux-x64-5.1.0.840.zip"
STAGED_COMMANDLINE_TOOLS_PATH=

usage() {
    echo "Usage: $0 [--sdk-path PATH]"
}

while [[ "$#" -gt 0 ]]
do
    case "$1" in
        --sdk-path)
            if [[ "$#" -lt 2 ]]
            then
                echo "Missing value for --sdk-path" >&2
                usage >&2
                exit 1
            fi
            COMMANDLINE_TOOLS_PATH="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

cleanup() {
    if [[ -n "${STAGED_COMMANDLINE_TOOLS_PATH}" && -f "${STAGED_COMMANDLINE_TOOLS_PATH}" ]]
    then
        rm -f "${STAGED_COMMANDLINE_TOOLS_PATH}"
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

# Build the helper images
"${CONTAINER_CLI}" build base -f base/Dockerfile -t "localhost/servo_gha_base:latest" --build-arg=USERNAME=${IMAGE_USERNAME}
"${CONTAINER_CLI}" build gh_runner -f gh_runner/Dockerfile -t "localhost/servo_gha_runner:${GITHUB_ACTIONS_RUNNER_VERSION}" \
    --build-arg=USERNAME=${IMAGE_USERNAME} \
    --build-arg=GITHUB_ACTIONS_RUNNER_VERSION=${GITHUB_ACTIONS_RUNNER_VERSION}
"${CONTAINER_CLI}" build hos_commandline_tools -f hos_commandline_tools/Dockerfile -t "localhost/hos_commandline_tools:latest" \
   --build-arg=USERNAME=${IMAGE_USERNAME} \
   "--build-arg=COMMANDLINE_TOOLS_PATH=${COMMANDLINE_TOOLS_PATH}"

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
    --build-arg USERNAME=${IMAGE_USERNAME}
