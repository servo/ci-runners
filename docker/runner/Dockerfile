ARG GITHUB_ACTIONS_RUNNER_VERSION
ARG HOS_COMMANDLINE_TOOLS_VERSION

FROM ubuntu:20.04 AS rust
RUN apt update && apt install -y curl build-essential
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain stable \
        --profile=minimal
RUN /root/.cargo/bin/cargo install hitrace-bench --version 0.7.0

FROM hos_commandline_tools:${HOS_COMMANDLINE_TOOLS_VERSION} AS commandline_tools

FROM servo_gha_runner:${GITHUB_ACTIONS_RUNNER_VERSION}

USER root
# Java is required for signing the hap, libatomic1 for hdc
RUN apt-get update && \
    apt-get -y install --no-install-recommends \
        openjdk-21-jre-headless libatomic1 libc6 build-essential jq \
    && rm -rf /var/cache/apt/lists

ARG USERNAME
USER ${USERNAME}
WORKDIR /data/servo/servo

# todo: We don't actually need the commandline tools. We could also just use the OH SDK.
COPY --from=commandline_tools /data/command-line-tools/sdk/default/openharmony/toolchains /data/commandline-tools/sdk/default/openharmony/toolchains

# Add signing material.
# Note: We could solve this via Github secrets, but that would require first auditing all
# servo workflows. I suspect that it is currently possible to extract secrets from the servo repo
# via pull requests with malicous build scripts.
ADD --chown=${USERNAME}:${USERNAME} ohos-config.tar /home/${USERNAME}/
# Used to authorize with the hdc device and avoid the confirmation dialog.
COPY --chown=${USERNAME}:${USERNAME} hdckey hdckey.pub /home/${USERNAME}/.harmony/
ADD hdc.tar /usr/bin/

COPY sign.sh /usr/bin/sign-hos.sh

COPY --from=rust /root/.cargo/bin /data/cargo
ENV PATH="/home/${USERNAME}/.local/bin:/data:/data/cargo:/data/commandline-tools/sdk/default/openharmony/toolchains/:${PATH}"

