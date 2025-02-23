from fastapi import FastAPI, Request, Response, status, HTTPException
import uvicorn
import os
from transformers import AutoImageProcessor, ResNetForImageClassification
import torch
from PIL import Image
import base64
from io import BytesIO

artefact_on_sagemaker = os.path.isdir("/opt/ml/model") and os.listdir("/opt/ml/model")
if artefact_on_sagemaker: 
    artefact_path = "/opt/ml/model"
else:
    artefact_path = "../artefact"

model = ResNetForImageClassification.from_pretrained(local_files_only=True, config="config.json", pretrained_model_name_or_path=artefact_path)
app = FastAPI()


@app.get('/ping')
async def ping():
    if model: 
        return Response(status_code=status.HTTP_200_OK)
    else:
        raise HTTPException(status_code=500, detail="Error")

@app.post('/invocations')
async def predict(request: Request):
    body = await request.json()
    processor = AutoImageProcessor.from_pretrained("preprocessor_config.json", use_fast=True)
    image = Image.open(BytesIO(base64.b64decode(body["image"])))
    inputs = processor(image, return_tensors="pt")
    with torch.no_grad():
        logits = model(**inputs).logits
    predicted_label = logits.argmax(-1).item()
    return {
        "label": model.config.id2label[predicted_label]
    }

if __name__ == "__main__":
    config = uvicorn.Config("serve:app", port=8080, host="0.0.0.0")
    server = uvicorn.Server(config=config)
    server.run()