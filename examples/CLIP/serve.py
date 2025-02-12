from fastapi import FastAPI, HTTPException

import os
from io import BytesIO
import base64

import torch
import clip 
from PIL import Image

import uvicorn

device = "cuda" if torch.cuda.is_available() else "cpu"

if os.getenv("SIMPLE_SAGE_ARTIFACT_PATH"):
    model, preprocess = clip.load(os.getenv("SIMPLE_SAGE_ARTIFACT_PATH"), device=device)
else:
    model, preprocess = clip.load("ViT-B/32", device=device)


app = FastAPI()

@app.post('/ping')
def ping():
    if model: 
        return 
    else:
        raise HTTPException(status_code=500, detail="Error")

@app.post('/invocations')
def predict(request):
    image = preprocess(Image.open(BytesIO(base64.b64decode(request.image)))).unsqueeze(0).to(device)
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
    config = uvicorn.Config("serve:app", port=8080)
    server = uvicorn.Server(config=config)
    server.run()

