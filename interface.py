import requests
from pathlib import Path

if __name__ == '__main__':
    db_url = "http://172.19.0.2:8081"
    captioner_url = "http://172.20.0.2:5000"

    images = [(image.name, image.read_bytes()) for image in Path("./sample_images").iterdir() if image.is_file()]
    ids = requests.post(f"{db_url}/upload_raw", files=images).json()["ids"]
    for im_id in ids:
        jpg = requests.get(f"{db_url}/fetch_jpg?id={im_id}&width=600&height=400&quality=90", stream=True).content
        print(len(jpg))
        print(requests.post(f"{captioner_url}/model/predict", files={"image": ("image.jpg", jpg, "image/jpeg", {})}).json())
