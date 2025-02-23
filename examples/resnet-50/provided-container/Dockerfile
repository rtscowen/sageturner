FROM python:3.12

# Python build dependencies 
RUN apt-get -y update && DEBIAN_FRONTEND=noninteractive apt-get -y install --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN python -m ensurepip

### install python packages
RUN pip install fastapi[standard]
RUN pip install --extra-index-url https://download.pytorch.org/whl/cpu transformers[torch]
RUN pip install pillow

### set env vars for sagemaker
ENV PYTHONUNBUFFERED=TRUE
ENV PYTHONDONTWRITEBYTECODE=TRUE
ENV PATH="${PATH}:/opt/program/"

### copy serve script
COPY . /opt/program
WORKDIR /opt/program

### set entrypoint; will get 'serve' as arg when started for inference
ENTRYPOINT [ "python", "serve.py" ]