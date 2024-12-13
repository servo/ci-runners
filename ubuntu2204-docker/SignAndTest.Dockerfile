FROM ubuntu:22.04 AS base_fetcher

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get -y install --no-install-recommends \
    ca-certificates unzip curl git jq libatomic1 \
    && apt-get clean

ARG USERNAME=servo_ci
ARG USER_UID=1000
ARG USER_GID=${USER_UID}

RUN groupadd --gid ${USER_GID} ${USERNAME} && \
    useradd -s /bin/bash --uid ${USER_UID} --gid ${USER_GID} -m ${USERNAME}


FROM base_fetcher AS gh_runner
USER ${USERNAME}
WORKDIR /home/${USERNAME}

ARG GITHUB_ACTIONS_RUNNER_VERSION=2.321.0
RUN curl -Lo actions-runner.tar.gz \
    https://github.com/actions/runner/releases/download/v${GITHUB_ACTIONS_RUNNER_VERSION}/actions-runner-linux-x64-${GITHUB_ACTIONS_RUNNER_VERSION}.tar.gz \
    && mkdir runner \
    && tar xzf actions-runner.tar.gz -C runner \
    && rm actions-runner.tar.gz

FROM base_fetcher AS commandline_tools

RUN mkdir -p data
ADD commandline-tools-linux-x64-5.0.3.906.zip /data/commandline-tools.zip
RUN cd data && unzip -q commandline-tools.zip

FROM base_fetcher AS servo_base
COPY --chown=${USERNAME}:${USERNAME} --from=gh_runner /home/${USERNAME}/runner /home/${USERNAME}/runner
RUN /home/${USERNAME}/runner/bin/installdependencies.sh

# Java is required for signing the hap
RUN apt-get update && \
    apt-get -y install --no-install-recommends \
        openjdk-11-jre-headless

USER ${USERNAME}
WORKDIR /data/servo/servo

# todo: We don't actually need the commandline tools. We could also just use the OH SDK.
COPY --from=commandline_tools /data/command-line-tools/sdk/default/openharmony/toolchains /data/commandline-tools/sdk/default/openharmony/toolchains

# Add signing material.
# Note: We could solve this via Github secrets, but that would require first auditing all
# servo workflows. I suspect that it is currently possible to extract secrets from the servo repo
# via pull requests with malicous build scripts.
ADD ohos-config.tar /home/${USERNAME}/
# Used to authorize with the hdc device and avoid the confirmation dialog.
COPY --chown=${USERNAME}:${USERNAME} hdckey hdckey.pub /home/${USERNAME}/.harmony/
ADD hdc.tar /usr/bin/

COPY sign.sh /usr/bin/sign-hos.sh

ENV PATH="/home/${USERNAME}/.local/bin:${PATH}"



