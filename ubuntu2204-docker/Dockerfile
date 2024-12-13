FROM ubuntu:22.04 AS base_fetcher

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get -y install --no-install-recommends \
    ca-certificates unzip curl git jq \
    && apt-get clean

ARG USERNAME=servo_ci
ARG USER_UID=1000
ARG USER_GID=${USER_UID}

RUN groupadd --gid ${USER_GID} ${USERNAME} && \
    useradd -s /bin/bash --uid ${USER_UID} --gid ${USER_GID} -m ${USERNAME}


FROM base_fetcher AS base_with_cc
RUN apt-get update && \
    apt-get -y install --no-install-recommends \
    build-essential ccache clang cmake python3 \
    && apt-get clean

FROM base_with_cc AS base


# Add apt-repository for `gh`
COPY --chmod=644 githubcli-archive-keyring.gpg /etc/apt/keyrings/githubcli-archive-keyring.gpg
RUN echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" > /etc/apt/sources.list.d/github-cli.list

RUN apt-get update && \
    apt-get -y install --no-install-recommends \
        libdbus-1-dev libfreetype6-dev libgl1-mesa-dri libgles2-mesa-dev \
        libglib2.0-dev gstreamer1.0-plugins-good libgstreamer-plugins-good1.0-dev \
        gstreamer1.0-plugins-bad libgstreamer-plugins-bad1.0-dev gstreamer1.0-plugins-ugly \
        gstreamer1.0-plugins-base libgstreamer-plugins-base1.0-dev gstreamer1.0-libav \
        libgstrtspserver-1.0-dev gstreamer1.0-tools libges-1.0-dev \
        libharfbuzz-dev liblzma-dev libudev-dev libunwind-dev \
        libvulkan1 libx11-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxmu-dev \
        libxmu6 libegl1-mesa-dev llvm-dev m4 xorg-dev libxkbcommon0 libxkbcommon-x11-0 \
        xvfb gh sudo

RUN mkdir -p /data/servo/servo && chown -R "${USERNAME}:${USERNAME}" /data/

FROM base_fetcher AS gh_runner
USER ${USERNAME}
WORKDIR /home/${USERNAME}

ARG GITHUB_ACTIONS_RUNNER_VERSION=2.321.0
RUN curl -Lo actions-runner.tar.gz \
    https://github.com/actions/runner/releases/download/v${GITHUB_ACTIONS_RUNNER_VERSION}/actions-runner-linux-x64-${GITHUB_ACTIONS_RUNNER_VERSION}.tar.gz \
    && mkdir runner \
    && tar xzf actions-runner.tar.gz -C runner \
    && rm actions-runner.tar.gz 

FROM base_with_cc AS rust_toolchain
USER ${USERNAME}
ARG RUST_VERSION=1.83.0
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --default-toolchain ${RUST_VERSION} \ 
        --profile=minimal \
        --component rustfmt clippy cargo llvm-tools rustc-dev rustdocs

# todo: delete registy
RUN /home/${USERNAME}/.cargo/bin/cargo install cargo-chef taplo-cli cargo-deny --locked

FROM base_fetcher AS uv

ARG UV_VERSION=0.5.10
RUN mkdir -p "$HOME/.local/bin" \
    && curl --proto '=https' --tlsv1.2  --fail \
        -LsSf https://github.com/astral-sh/uv/releases/download/${UV_VERSION}/uv-installer.sh | sh

##### FETCH SERVO ####################################################################
FROM base_fetcher AS servo_fetcher 

RUN mkdir -p /data/servo/servo && chown -R "${USERNAME}:${USERNAME}" /data/
USER ${USERNAME}
# Clone servo, ignoring the history. We keep a small history, just in case some of our 
# scripts might rely on a bit of history being available.
RUN cd /data/servo && git clone https://github.com/servo/servo.git --depth 10
WORKDIR /data/servo/servo

# The above layer will be cached. For faster image rebuilds we 
# just pull the diff from the previous cached layer.
# todo: inject this as a build arg based on latest current commit!
ARG SERVO_GIT_HASH=origin/main
RUN git pull && git reset --hard "${SERVO_GIT_HASH}"

##### END FETCH SERVO #################################################################

FROM base_fetcher AS commandline_tools

RUN mkdir -p data
ADD commandline-tools-linux-x64-5.0.3.906.zip /data/commandline-tools.zip
# Todo: Can we perhaps delete the parts we don't need (e.g. target triples etc.)
RUN cd data && unzip -q commandline-tools.zip

FROM base AS servo_base
COPY --chown=${USERNAME}:${USERNAME} --from=gh_runner /home/${USERNAME}/runner /home/${USERNAME}/runner
RUN /home/${USERNAME}/runner/bin/installdependencies.sh

# only needed for 
RUN apt-get update && \
    apt-get -y install --no-install-recommends \
        openjdk-11-jre-headless

USER ${USERNAME}
WORKDIR /data/servo/servo

COPY --chown=${USERNAME}:${USERNAME} --from=rust_toolchain /home/${USERNAME}/.cargo /home/${USERNAME}/.cargo 
COPY --chown=${USERNAME}:${USERNAME} --from=rust_toolchain /home/${USERNAME}/.rustup /home/${USERNAME}/.rustup
COPY --chown="${USERNAME}:${USERNAME}" --from=uv /root/.local/bin/uv /root/.local/bin/uvx /home/${USERNAME}/.cargo/bin/
ENV PATH="/home/${USERNAME}/.cargo/bin:${PATH}"

COPY --from=servo_fetcher /data/servo /data/servo
COPY --chown=${USERNAME}:${USERNAME} --from=commandline_tools /data/command-line-tools /data/commandline-tools

# Set some required environment variables for building HarmonyOS and OpenHarmony
ENV DEVECO_SDK_HOME=/data/commandline-tools/sdk
ENV NODE_HOME=/data/commandline-tools/tool/node
ENV PATH=/home/${USERNAME}/.local/bin:${PATH}:/data/commandline-tools/hvigor/bin/:${NODE_HOME}/bin
ENV OHOS_BASE_SDK_HOME=/data/commandline-tools/sdk/default/openharmony
ENV OHOS_SDK_NATIVE=/data/commandline-tools/sdk/default/openharmony/native

# ideally this should be a no-op: Todo: investigate the layer and see if it is.
RUN cd /data/servo/servo && ./mach bootstrap --skip-platform


FROM servo_base AS servo_cooked_ohos_release 

RUN ./mach build --ohos --profile=release --no-package --no-default-features --features=tracing-hitrace 
