import requests
from pathlib import Path

if __name__ == '__main__':
    images = [(image.name, image.read_bytes()) for image in Path("./sample_images").iterdir() if image.is_file()]
    print(requests.post("http://127.0.0.1:8081/upload_raw", files=images).json())
