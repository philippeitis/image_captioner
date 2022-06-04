import React, {useState, useCallback} from 'react';
import ImageResult from "./ImageResult";

function App() {
  const [results, setResults] = useState({});
  const [searchTerm, setSearchTerm] = useState('');

  const onChange = event => {
    setSearchTerm(event.target.value);
  };

  const search = useCallback(async () => {
      const res = await fetch(`https://localhost/near_text?text=${searchTerm}`);
      if (res.ok) {
          const response_json = await res.json();
          setResults(response_json.ids);
      } else {
          setResults([]);
      }
  }, [searchTerm]);

  const onSubmit = event => {
    search();
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
      {results.length > 0 && <ImageResult id={results[0]} />}
    </div>
  );
}

export default App;
