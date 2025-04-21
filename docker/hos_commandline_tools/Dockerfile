FROM servo_gha_base:latest

RUN mkdir -p data
ARG COMMANDLINE_TOOLS_PATH=https://repo.huaweicloud.com/harmonyos/ohpm/5.0.5/commandline-tools-linux-x64-5.0.5.310.zip
ADD ${COMMANDLINE_TOOLS_PATH} /data/commandline-tools.zip
RUN cd data && unzip -q commandline-tools.zip
