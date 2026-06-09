#!/usr/bin/env bash

set -eu

COMMANDLINE_TOOLS_PATH="https://repo.huaweicloud.com/harmonyos/ohpm/5.1.0/commandline-tools-linux-x64-5.1.0.840.zip"
STAGED_COMMANDLINE_TOOLS_PATH=
USER_SPECIFIED_SDK_PATH=0

sdk_version() {
    local sdk_path
    sdk_path="${1##*/}"

    if [[ "${sdk_path}" =~ commandline-tools-linux-x64-(.+)\.zip$ ]]
    then
        echo "${BASH_REMATCH[1]}"
    else
        echo "${sdk_path}"
    fi
}

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
            USER_SPECIFIED_SDK_PATH=1
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

if [[ "${USER_SPECIFIED_SDK_PATH}" -eq 1 && "${COMMANDLINE_TOOLS_PATH}" != http://* && "${COMMANDLINE_TOOLS_PATH}" != https://* && ! -f "${COMMANDLINE_TOOLS_PATH}" ]]
then
    echo "SDK archive not found: ${COMMANDLINE_TOOLS_PATH}" >&2
    exit 1
fi

cleanup() {
    if [[ -n "${STAGED_COMMANDLINE_TOOLS_PATH}" && -f "${STAGED_COMMANDLINE_TOOLS_PATH}" ]]
    then
        rm -f "${STAGED_COMMANDLINE_TOOLS_PATH}"
    fi
}

trap cleanup EXIT

if [[ -f "${COMMANDLINE_TOOLS_PATH}" ]]
then
    echo "Using SDK version $(sdk_version "${COMMANDLINE_TOOLS_PATH}") from local archive: ${COMMANDLINE_TOOLS_PATH}"
else
    echo "Using SDK version $(sdk_version "${COMMANDLINE_TOOLS_PATH}") from web"
fi

if [[ -f "${COMMANDLINE_TOOLS_PATH}" ]]
then
    STAGED_COMMANDLINE_TOOLS_PATH="hos_commandline_tools/.commandline-tools.zip"
    cp "${COMMANDLINE_TOOLS_PATH}" "${STAGED_COMMANDLINE_TOOLS_PATH}"
    COMMANDLINE_TOOLS_PATH=".commandline-tools.zip"
fi

SERVO_GIT_HASH=$(git ls-remote https://github.com/servo/servo.git --branches refs/heads/main | awk '{ print $1}')
GITHUB_ACTIONS_RUNNER_VERSION="2.335.1"
MITMPROXY_VERSION="12.2.1"
RUST_VERSION="1.95.0"
UV_VERSION="0.11.19"
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
   --build-arg=USERNAME=${IMAGE_USERNAME} \
   "--build-arg=COMMANDLINE_TOOLS_PATH=${COMMANDLINE_TOOLS_PATH}"

# Build the actual images

docker build hos_builder -f hos_builder/Dockerfile -t servo_gha_hos_builder:latest \
     --build-arg SERVO_GIT_HASH=${SERVO_GIT_HASH} \
     --build-arg HOS_COMMANDLINE_TOOLS_VERSION=latest \
     --build-arg GITHUB_ACTIONS_RUNNER_VERSION=${GITHUB_ACTIONS_RUNNER_VERSION} \
     --build-arg RUST_VERSION=${RUST_VERSION} \
     --build-arg UV_VERSION=${UV_VERSION} \
     --build-arg USERNAME=${IMAGE_USERNAME}

docker build runner -f runner/Dockerfile -t servo_gha_hos_runner:latest \
    --build-arg HOS_COMMANDLINE_TOOLS_VERSION=latest \
    --build-arg GITHUB_ACTIONS_RUNNER_VERSION=${GITHUB_ACTIONS_RUNNER_VERSION} \
    --build-arg MITMPROXY_VERSION=${MITMPROXY_VERSION} \
    --build-arg UV_VERSION=${UV_VERSION} \
    --build-arg USERNAME=${IMAGE_USERNAME}
