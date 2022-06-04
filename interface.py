import requests
import weaviate

from pathlib import Path
import time

import os
os.environ["REQUESTS_CA_BUNDLE"] = './certs/test.pem'
os.environ["SSL_CERT_FILE"] = './certs/test.pem'

schema = {
    "classes": [{
        "class": "ClipImage",
        "vectorIndexType": "hnsw",
        "vectorizer": "none",
        "properties": [
            # TODO: https://weaviate.io/developers/weaviate/current/data-schema/datatypes.html#datatype-geocoordinates
            # {
            #   "dataType": [
            #       "geoCoordinates"
            #   ],
            #   "description": "Geo location of the HQ",
            #   "name": "headquartersGeoLocation"
            # }
        ]
    }]
}

if __name__ == '__main__':
    db_url = "https://localhost"
    Path("./preview").mkdir(exist_ok=True)

    client = weaviate.Client("https://localhost")
    #
    # try:
    #     client.schema.delete_class("ClipImage")
    #     print("Schema deleted")
    # except weaviate.exceptions.UnexpectedStatusCodeException as e:
    #     print(e)
    # client.schema.create(schema)
    # print("Schema defined")

    start = time.time()
    images = [(image.name, image.read_bytes()) for image in Path("./sample_images").iterdir() if image.is_file()]
    ids = requests.post(f"{db_url}/upload_raw", files=images).json()
    print(ids)
    end = time.time()
    print(f"Images uploaded in {end - start}s")

    query_result = client.query \
        .get("ClipImage", ["_additional {certainty id} "]) \
        .with_near_text(
        {
            'concepts': ["bird in tree"],
            "properties": ["image"],
        }
    ) \
        .do()

    print(query_result)
