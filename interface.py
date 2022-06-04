import os
import time
from pathlib import Path

import requests

os.environ["REQUESTS_CA_BUNDLE"] = './certs/test.pem'
os.environ["SSL_CERT_FILE"] = './certs/test.pem'

DB_URL = "https://localhost"

# TODO: https://weaviate.io/developers/weaviate/current/data-schema/datatypes.html#datatype-geocoordinates
# {
#   "dataType": [
#       "geoCoordinates"
#   ],
#   "description": "Geo location of the HQ",
#   "name": "headquartersGeoLocation"
# }

if __name__ == '__main__':

    start = time.time()
    images = [(image.name, image.read_bytes()) for image in Path("./sample_images").iterdir() if image.is_file()]
    ids = requests.post(f"{DB_URL}/upload_raw", files=images).json()
    print(ids)
    end = time.time()
    print(f"Images uploaded in {end - start}s")

    print("response:", requests.get(f"{DB_URL}/near_text?text=cat").text)
