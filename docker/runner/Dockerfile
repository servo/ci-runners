## syntax=docker/dockerfile:1.4

ARG GITHUB_ACTIONS_RUNNER_VERSION=2.333.0

FROM ubuntu:24.04 AS rust
RUN apt update && apt install -y curl build-essential
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable \
        --profile=minimal
RUN /root/.cargo/bin/cargo install hitrace-bench --version 0.11.3

FROM localhost/hos_commandline_tools:latest AS commandline_tools

FROM localhost/servo_gha_runner:${GITHUB_ACTIONS_RUNNER_VERSION}

USER root
# Java is required for signing the hap, libatomic1 for hdc
RUN apt-get update && \
    apt-get -y install --no-install-recommends \
        openjdk-21-jre-headless libatomic1 libc6 build-essential jq python3 \
    && rm -rf /var/cache/apt/lists

ARG USERNAME
ARG SKIP_HDC_KEY=0
USER ${USERNAME}
WORKDIR /data/servo/servo

# todo: We don't actually need the commandline tools. We could also just use the OH SDK.
COPY --from=commandline_tools /data/command-line-tools/sdk/default/openharmony/toolchains /data/commandline-tools/sdk/default/openharmony/toolchains

# Add signing material.
# Note: We could solve this via Github secrets, but that would require first auditing all
# servo workflows. I suspect that it is currently possible to extract secrets from the servo repo
# via pull requests with malicous build scripts.
ADD --chown=${USERNAME}:${USERNAME} ohos-signing-config.tar.gz /home/${USERNAME}/
# According to https://github.com/moby/moby/issues/35525 the chown flag does not chown the files inside the archive.
USER root
RUN chown ${USERNAME}:${USERNAME} -R /home/${USERNAME}/.ohos
USER ${USERNAME}

# Add for tar.gz keeps the permission
RUN chown ${USERNAME}:${USERNAME} -R /home/${USERNAME}/.ohos

# Some SDK versions still expect the legacy authorization key pair.
USER root
RUN --mount=type=bind,source=.staged_hdckey,target=/tmp/hdckey,readonly \
    --mount=type=bind,source=.staged_hdckey.pub,target=/tmp/hdckey.pub,readonly \
    mkdir -p /home/${USERNAME}/.harmony && \
    if [ "${SKIP_HDC_KEY}" != "1" ]; then \
        cp /tmp/hdckey /home/${USERNAME}/.harmony/hdckey && \
        cp /tmp/hdckey.pub /home/${USERNAME}/.harmony/hdckey.pub; \
    fi && \
    chown ${USERNAME}:${USERNAME} -R /home/${USERNAME}/.harmony
USER ${USERNAME}

COPY sign.sh /usr/bin/sign-hos.sh

ARG MITMPROXY_VERSION
ARG UV_VERSION
RUN curl -LsSf https://astral.sh/uv/${UV_VERSION}/install.sh | sh
RUN /home/${USERNAME}/.local/bin/uv tool install mitmproxy@${MITMPROXY_VERSION}
COPY --from=rust /root/.cargo/bin /data/cargo
ENV PATH="/home/${USERNAME}/.local/bin:/data/cargo:/data/commandline-tools/sdk/default/openharmony/toolchains/:${PATH}"
