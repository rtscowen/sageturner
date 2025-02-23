# Sageturner - 0.1.0

Sageturner is a simple binary CLI application, designed to be the easiest way to deploy models to AWS Sagemaker. It can setup a sensible Sagemaker role and bucket for you (supply your own if you prefer).

It's distributed as a 22mb binary application. You can use it standalone, or as part of a CI/CD pipeline. 

For people who don't want to mess around with Dockerfiles, Sageturner is capable of auto generating sensible containers for you (use the `--container-mode generate` flag to `sageturner deploy` instead of `--container-mode provided`). Note: you will run into CUDA hell eventually, though the examples in this repo (including CLIP) do work. I will be actively improving this functionality to cover more use cases (raise an issue if you want me to cover yours). You can also just provide your own Dockerfile and it'll build and push it as is, if generation fails for you.

# Installation

Sageturner is distributed as a binary. Checkout the releases page of the repo, and download the appropriate release for your platform. 

# Pre-requisites 

Sageturner requires the following: 
- Docker : it talks to the Docker daemon on your system, so you need Docker installed and running
- AWS credentials : To talk to Sagemaker, we use the Rust Sagemaker SDK. You need to make sure you have run `aws configure` so that Sageturner can find appropriate credentials. The credentials you autheticate with to AWS 
must be able to: create and uplaod to S3 buckets, create and describe IAM roles, create repos on ECR. Sageturner will give you helpful error messages if you don't have these. If you're stuck, drop me a message or open an issue.

# Getting Started

**tl;dr**: run `sageturner --help` or just `sageturner` for a list of commands (and `sageturner [command] --help` to see that commands parameters), then head over to the examples. Start with the resnet example, as it's simpler, and look at the sageturner.yaml
to see the config driven approach. 

## Command list 

### setup

`sageturner setup` takes no parameters, and creates an S3 bucket called sageturner-sagemaker-models and an execution role (Sagemaker needs this to work properly) called sageturner-role-sagemaker. If you want to create your own bucket and roles, you can 
easily override these defaults in your sageturner.yaml

### deploy

`sageturner deploy` is where things get interesting. You can either read the below, or dive right in by running one of the examples: to deploy ResNet50 to a serverless endpoint - after running setup - try running `sageturner deploy --endpoint-type serverless --container-mode generate --config-path ./examples/resnet50/sageturner.yaml` from the root of the repo to generate a container for resnet50, and deploy it to a serverless endpoint. then take a look at sageturner.yaml and see the comments for an explanation of what's going on.

Let's look at each of the flags, you need all of them: 

#### --container-mode

You have two options here. 
- generate : sageturner makes a sensible container for you (or tries to, you may end up in CUDA hell eventually, I'm working on it).
- provide : provide your own Dockerfile. you must meet the sagemaker requirements for serving if you do it this way: checkout serve.py and the Dockerfile in the provided-container dirs of the example. Briefly, you need to respond to GETs on
/ping, and POSTs on /invocations

If you pick generate container, the tool expects you to define a load() and predict() method in a file called sageturner.py that explain how to load and predict your model. There's a whole section in the config, see the resnet example, 
with what you need to provide. Sageturner then wires everything together for you. 

#### --endpoint-type

 Again, two options, pretty self-explanatory: 

- serverless : deploy to an AWS sagemaker serverless endpoint. Note two things: no GPUs on sagemaker serverless inference, and strict 10gb image limit (images near the border of 10gb can also be refused). If you're using --container-mode generate
in combo with serverless, sageturner won't allow the deployment if you've opted for install_cuda in your generate_container config in the YAML. I'll be adding checks on image size in a future version.
- server : deploy to an AWS provisioned endpoint. you'll need to supply instance type etc in the config file

#### --config-path

Absolute or relative path to sageturner.yaml 

The best way to understand these fields is the Resnet example, which has detailed comments.

# A note on sageturner.py : the file you *must* provide for generated containers

If you want Sageturner to auto-generate a sensible container for you, you need to provide a file called sageturner.py in the code_dir

Sageturner acts on the fields in your config, and generates a FastAPI model server for you. but it needs you to tell it how to load your model,
and how to run inference. 

This file needs two methods, with the following signature: 

```
def load():
    # load your model. see the Resnet example, and then the CLIP example for a slightly more complicated case with the CLIP preprocessor
    return model
```

```
def predict(model, request):
    # receives the model you created in load(), and additional a dictionary of the JSON body
    # of the request the endpoint was invoked with
    # access fields like request["image"] 
    # call it like model(base_64_decoded_image) 
    # Return a dict with whatever you want your response to be.
    return {

    }
```


