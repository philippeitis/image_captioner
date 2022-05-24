import React, {useState, useCallback} from 'react';
import logo from './logo.svg';

const weaviate = require('weaviate-client');

const client = weaviate.client({
  scheme: 'http',
  host: 'localhost:8080',
});

function App() {
  const [results, setResults] = useState({});
  const [searchTerm, setSearchTerm] = useState('');

  const onChange = event => {
    setSearchTerm(event.target.value);
  };

  const fetch = useCallback(async () => {
      const res = await client.graphql
        .get()
        .withClassName('ClipImage')
        .withNearText({concepts: [searchTerm]})
        .withFields('image _additional { id }')
        .withLimit(1)
        .do();
      const images = res["data"]["Get"]["ClipImage"]
          .map(x => ({ id: x._additional.id , image: x.image }));
      console.log(images);

      setResults(images);
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
        <div className="field has-addons">
          <div className="control is-expanded">
            <input
              className="input is-large"
              type="text"
              placeholder="Search for images"
              onChange={onChange}
            />
          </div>
          <div className="control">
            <input
              type="submit"
              className="button is-info is-large"
              value="Search"
              style={{backgroundColor: '#fa0171'}}
            />
          </div>
        </div>
      </form>
      {results.length > 0 && (
        <img
          width="100%"
          alt="Multi-Modal Search Result"
          src={
            'data:image/jpg;base64,' +
            results[0].image
          }
        />
      )}
    </div>
  );
}

export default App;
