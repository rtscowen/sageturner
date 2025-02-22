FROM ubuntu:22.04

SHELL ["/bin/bash", "-c"]

# Python build dependencies 
RUN apt-get -y update && DEBIAN_FRONTEND=noninteractive apt-get -y install --no-install-recommends \ 
    build-essential libssl-dev zlib1g-dev \
    libbz2-dev libreadline-dev libsqlite3-dev curl git \
    libncursesw5-dev xz-utils tk-dev libxml2-dev libxmlsec1-dev libffi-dev liblzma-dev wget

# Pyenv 
ENV HOME=/home/root 
RUN curl -fsSL https://pyenv.run | sh
ENV PYENV_ROOT=${HOME}/.pyenv
ENV PATH=${PYENV_ROOT}/shims:${PYENV_ROOT}/bin:$PATH

# Python
RUN pyenv --version
RUN pyenv install 3.12
RUN pyenv global 3.12

### install python packages
RUN pip install fastapi[standard]
RUN pip install ftfy regex tqdm
RUN pip install git+https://github.com/openai/CLIP.git

### set env vars for sagemaker
ENV PYTHONUNBUFFERED=TRUE
ENV PYTHONDONTWRITEBYTECODE=TRUE
ENV PATH="${PATH}:/opt/program/"

### copy serve script
COPY serve.py /opt/program/
WORKDIR /opt/program/

### set entrypoint; will get 'serve' as arg when started for inference. serve.py deals with 
ENTRYPOINT [ "python", "serve.py" ]