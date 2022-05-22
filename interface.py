import requests
import weaviate

from pathlib import Path
import base64

schema = {
    "classes": [{
        "class": "ClipImage",
        "moduleConfig": {
            "multi2vec-clip": {
                "imageFields": [
                    "image"
                ],
                # "textFields": [],
                # "weights": {
                #     "textFields": [0.],
                #     "imageFields": [1.0]
                # }
            }
        },
        "vectorIndexType": "hnsw",
        "vectorizer": "multi2vec-clip",
        "properties": [
            {
                "dataType": [
                    "blob"
                ],
                "name": "image"
            },
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
    db_url = "http://172.27.0.2:8081"
    captioner_url = "http://172.28.0.3:5000"
    Path("./preview").mkdir(exist_ok=True)

    client = weaviate.Client("http://localhost:8080")  # or another location where your Weaviate instance is running

    try:
        client.schema.delete_class("ClipImage")
        print("Schema deleted")
    except weaviate.exceptions.UnexpectedStatusCodeException as e:
        print(e)
    client.schema.create(schema)
    print("Schema defined")

    images = [(image.name, image.read_bytes()) for image in Path("./sample_images").iterdir() if image.is_file()]
    ids = requests.post(f"{db_url}/upload_raw", files=images).json()["ids"]

    print("Images uploaded")

    for im_id, (image, _) in zip(ids, images):
        jpg = requests.get(f"{db_url}/fetch_jpg?id={im_id}&width=600&height=400&quality=90", stream=True).content
        print(image)

        client.data_object.create({"image": base64.b64encode(jpg).decode("utf-8")}, "ClipImage", uuid=im_id)
        (Path("./preview") / Path(image).with_suffix(".jpg")).write_bytes(jpg)

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
