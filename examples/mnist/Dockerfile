FROM ubuntu:22.04

SHELL ["/bin/bash", "-c"]

COPY serve.py /opt/program/
WORKDIR /opt/program/

RUN apt-get -y update

ENTRYPOINT [ "python", "serve.py" ]