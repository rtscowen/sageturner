from fastapi import FastAPI, HTTPException, Response, status, Request

import os
from io import BytesIO
import base64

import torch
import clip 
from PIL import Image

import uvicorn

device = "cuda" if torch.cuda.is_available() else "cpu"

artefact_on_sagemaker = os.path.isdir("/opt/ml/model") and os.listdir("/opt/ml/model")
if artefact_on_sagemaker:
    model, preprocess = clip.load("/opt/ml/model/ViT-B-32.pt", device=device)
else:
    model, preprocess = clip.load("ViT-B/32", device=device)


app = FastAPI()

@app.get('/ping')
async def ping():
    if model: 
        return Response(status_code=status.HTTP_200_OK)
    else:
        raise HTTPException(status_code=500, detail="Error")

@app.post('/invocations')
async def predict(request: Request):
    body =  await request.json()
    image = preprocess(Image.open(BytesIO(base64.b64decode(body["image"])))).unsqueeze(0).to(device)
    text = clip.tokenize(["a diagram", "a dog", "a beanbag"]).to(device)

    with torch.no_grad():
        image_features = model.encode_image(image)
        text_features = model.encode_text(text)

        logits_per_image, logits_per_text = model(image, text)
        probs = logits_per_image.softmax(dim=-1).cpu().numpy()

    return {
        "probs": probs.tolist()
    }

if __name__ == "__main__":
    config = uvicorn.Config("serve:app", port=8080, host="0.0.0.0")
    server = uvicorn.Server(config=config)
    server.run()

