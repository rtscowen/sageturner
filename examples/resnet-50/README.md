Toy example, using Sageturner to deploy resnet-50 

## Serverless deployment, Generate container according to settings in generate_container section of YAML

`sageturner deploy --endpoint-type serverless --container-mode generate --config-path sageturner.yaml`

## Provisioned deployment, container provided

`sageturner deploy --endpoint-type serverless --container-mode provide --config-path sageturner.yaml`

## Server deploy, container provided

`sageturner deploy --endpoint-type server --container-mode provide --config-path sageturner.yaml`

## Server deploy, container generated

`sageturner deploy --endpoint-type server --container-mode generate --config-path sageturner.yaml`