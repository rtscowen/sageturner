import os
from io import BytesIO
import base64

import torch
import clip 
from PIL import Image

# Don't change the signatures !!
def load():
    # artefact_on_sagemaker disitnguishes whether the code is running on Sagemaker or locally 
    artefact_on_sagemaker = os.path.isdir("/opt/ml/model") and os.listdir("/opt/ml/model")
    device = "cuda" if torch.cuda.is_available() else "cpu"
    # You can return the model and preprocessor in a dict and use the keys in predict
    if artefact_on_sagemaker:
        model, preprocess = clip.load("/opt/ml/model/ViT-B-32.pt", device=device)
    else:
        model, preprocess = clip.load("ViT-B/32", device=device)

    model_dict = {
        "model": model,
        "preprocess": preprocess,
        "device": device
    }
    return model

def predict(model, request):
    image = model["preprocess"](Image.open(BytesIO(base64.b64decode(request["image"])))).unsqueeze(0).to(model["device"])
    text = clip.tokenize(["a diagram", "a dog", "a beanbag"]).to(model["device"])

    with torch.no_grad():
        image_features = model["model"].encode_image(image)
        text_features = model["model"].encode_text(text)

        logits_per_image, logits_per_text = model["model"](image, text)
        probs = logits_per_image.softmax(dim=-1).cpu().numpy()
    
    return {
        "probs": probs.tolist()
    }


if __name__ == "__main__":
    # Convenient for local testing of this script before it's assembled into FastAPI server
    # by auto container mode. Follows the same flow that will happen for inference in the 
    # live server

    model = load()

    # prepare a fake request, base64 encoding the image of the CLIP diagram
    diagram = Image.open("clip.png")
    img_byte_array = BytesIO()
    diagram.save(img_byte_array, format="PNG")
    diagram_base64 = base64.b64encode(img_byte_array.getvalue()).decode("utf-8")

    request = {
        "image": diagram_base64
    }

    # This is how predict will be called in the generated container
    response = predict(model, request)
    print(response)