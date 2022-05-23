import React, {useState, useCallback} from 'react';
import ImageResult from "./ImageResult";

const weaviate = require('weaviate-client');

const client = weaviate.client({
  scheme: 'http',
  host: 'localhost:8080',
});

function App() {
  const [results, setResults] = useState([]);
  const [searchTerm, setSearchTerm] = useState('');

  const onChange = event => {
    setSearchTerm(event.target.value);
  };

  const fetch = useCallback(() => {
    async function fetch() {
      const res = await client.graphql
        .get()
        .withClassName('ClipImage')
        .withNearText({concepts: [searchTerm]})
        .withFields('image _additional { id }')
        .withLimit(1)
        .do();
      const images = res["data"]["Get"]["ClipImage"];
      console.log(images);
      setResults(images.map(x => ({ id: x._additional.id , image: x.image })));
    }

    fetch();
  }, [searchTerm]);

  const onSubmit = event => {
    fetch();
    event.preventDefault();
  };

  return (
    <div className="container" style={{textAlign: 'center'}}>
      <form
        onSubmit={onSubmit}
        style={{marginTop: '50px', marginBottom: '50px'}}
      >
        <div class="field has-addons">
          <div class="control is-expanded">
            <input
              class="input is-large"
              type="text"
              placeholder="Search for images"
              onChange={onChange}
            />
          </div>
          <div class="control">
            <input
              type="submit"
              class="button is-info is-large"
              value="Search"
              style={{backgroundColor: '#fa0171'}}
            />
          </div>
        </div>
      </form>
      {results.length > 0 && <ImageResult id={results[0].id} image={results[0].image} />}
    </div>
  );
}

export default App;
