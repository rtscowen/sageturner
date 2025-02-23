FROM ubuntu:22.04

SHELL ["/bin/bash", "-c"]

# Python build dependencies 
RUN apt-get -y update && DEBIAN_FRONTEND=noninteractive apt-get -y install --no-install-recommends \ 
    build-essential libssl-dev zlib1g-dev \
    libbz2-dev libreadline-dev libsqlite3-dev curl git \
    libncursesw5-dev xz-utils tk-dev libxml2-dev libxmlsec1-dev libffi-dev liblzma-dev wget

## Cuda toolkit
RUN wget https://developer.download.nvidia.com/compute/cuda/repos/ubuntu2204/x86_64/cuda-ubuntu2204.pin --no-check-certificate && \
     mv cuda-ubuntu2204.pin /etc/apt/preferences.d/cuda-repository-pin-600 && \
     wget https://developer.download.nvidia.com/compute/cuda/12.8.0/local_installers/cuda-repo-ubuntu2204-12-8-local_12.8.0-570.86.10-1_amd64.deb --no-check-certificate && \
     dpkg -i cuda-repo-ubuntu2204-12-8-local_12.8.0-570.86.10-1_amd64.deb && \
     cp /var/cuda-repo-ubuntu2204-12-8-local/cuda-*-keyring.gpg /usr/share/keyrings/ && \
     apt-get -y update && apt-get -y install cuda-toolkit-12-8

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
# CLIP specific packages
RUN pip install ftfy regex tqdm
RUN pip install git+https://github.com/openai/CLIP.git

### set env vars for sagemaker
ENV PYTHONUNBUFFERED=TRUE
ENV PYTHONDONTWRITEBYTECODE=TRUE
ENV PATH="${PATH}:/opt/program/"

### copy serve script
COPY . /opt/program
WORKDIR /opt/program

### set entrypoint; will get 'serve' as arg when started for inference. serve.py deals with 
ENTRYPOINT [ "python", "serve.py" ]






