	

# syntax=docker/dockerfile:1
# check=error=true

# This image serves as a minimal base for our multi-stage docker builds.
# This image is kept as lean as possible, so that download stages can start
# quickly as possible.
# We already add a user in this image, so all other images inherit it.

FROM ubuntu:22.04 AS base_fetcher

ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get -y install --no-install-recommends \
    ca-certificates unzip curl git jq \
    && rm -rf /var/cache/apt/lists

ARG USERNAME=servo_ci
ARG USER_UID=1000
ARG USER_GID=${USER_UID}

RUN groupadd --gid ${USER_GID} ${USERNAME} && \
    useradd -s /bin/bash --uid ${USER_UID} --gid ${USER_GID} -m ${USERNAME}
