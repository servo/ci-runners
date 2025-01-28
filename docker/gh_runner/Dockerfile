FROM servo_gha_base:latest

ARG USERNAME
USER ${USERNAME}
WORKDIR /home/${USERNAME}

ARG GITHUB_ACTIONS_RUNNER_VERSION
RUN curl -Lo actions-runner.tar.gz \
    https://github.com/actions/runner/releases/download/v${GITHUB_ACTIONS_RUNNER_VERSION}/actions-runner-linux-x64-${GITHUB_ACTIONS_RUNNER_VERSION}.tar.gz \
    && mkdir runner \
    && tar xzf actions-runner.tar.gz -C runner \
    && rm actions-runner.tar.gz 

USER root
RUN /home/${USERNAME}/runner/bin/installdependencies.sh \
    && rm -rf /var/cache/apt/lists