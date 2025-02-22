import base64
from io import BytesIO
import os

from PIL import Image
import torch
from transformers import AutoImageProcessor, ResNetForImageClassification

# Don't change the signatures !!
def load():
    # This load function assumes you provided a .tar.gz compressed artefact in your sageturner.yaml. If you did, 
    # Sagemaker makes the uncompressed tar.gz available at /opt/ml/model
    # If you don't provide an artefact, your load function will look different. I'd suggest you write it in a way
    # that lets you test the script locally and emulate the live from (see the if __main__ section). It either needs
    # to work the same on sagemaker and locally, or you need some way of distinguishing the cases
    # analogous to the artefact_on_sagemaker var below

    # artefact_on_sagemaker disitnguishes whether the code is running on Sagemaker or locally 
    artefact_on_sagemaker = os.path.isdir("/opt/ml/model") and os.listdir("/opt/ml/model")
    if artefact_on_sagemaker: 
        artefact_path = "/opt/ml/model"
    else:
        artefact_path = "../artefact"

    model = ResNetForImageClassification.from_pretrained(local_files_only=True, config="config.json", pretrained_model_name_or_path=artefact_path)
    return model

def predict(model, request):
    # Sageturner container gen includes any files in the generate_container directory 
    # so you can easily include things like HuggingFace preprocessor_configs for your convenience 
    processor = AutoImageProcessor.from_pretrained("preprocessor_config.json", use_fast=True)

    ## Expects an {"image": "BASE_64_IMAGE"} request payload
    image = Image.open(BytesIO(base64.b64decode(request["image"])))
    inputs = processor(image, return_tensors="pt")

    with torch.no_grad():
        logits = model(**inputs).logits
    
    predicted_label = logits.argmax(-1).item()

    return {
        "label": model.config.id2label[predicted_label]
    }

if __name__ == "__main__":
    # Convenient for local testing of this script before it's assembled into FastAPI server
    # by auto container mode. Follows the same flow that will happen for inference in the 
    # live server

    model = load()

    # prepare a fake request, base64 encoding the image of a majestic lab
    majestic_lab = Image.open("lab.jpg")
    img_byte_array = BytesIO()
    majestic_lab.save(img_byte_array, format="JPEG")
    majestic_lab_base64 = base64.b64encode(img_byte_array.getvalue()).decode("utf-8")

    request = {
        "image": majestic_lab_base64
    }

    # This is how predict will be called in the generated container
    response = predict(model, request)
    print(response)