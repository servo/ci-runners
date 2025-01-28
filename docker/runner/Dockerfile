ARG GITHUB_ACTIONS_RUNNER_VERSION
ARG HOS_COMMANDLINE_TOOLS_VERSION

FROM hos_commandline_tools:${HOS_COMMANDLINE_TOOLS_VERSION} AS commandline_tools

FROM servo_gha_runner:${GITHUB_ACTIONS_RUNNER_VERSION}

USER root
# Java is required for signing the hap
RUN apt-get update && \
    apt-get -y install --no-install-recommends \
        openjdk-11-jre-headless \
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
ADD ohos-config.tar /home/${USERNAME}/
# Used to authorize with the hdc device and avoid the confirmation dialog.
COPY --chown=${USERNAME}:${USERNAME} hdckey hdckey.pub /home/${USERNAME}/.harmony/
ADD hdc.tar /usr/bin/

COPY sign.sh /usr/bin/sign-hos.sh

ENV PATH="/home/${USERNAME}/.local/bin:${PATH}"

