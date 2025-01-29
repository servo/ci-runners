FROM servo_gha_base:latest

RUN mkdir -p data
ADD commandline-tools-linux-x64-5.0.3.906.zip /data/commandline-tools.zip
RUN cd data && unzip -q commandline-tools.zip
