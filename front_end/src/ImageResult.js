import React from 'react';
import { ReactComponent as DownloadIcon } from './save-file.svg';

class ImageResult extends React.Component {
    constructor(props) {
        super(props);
    // This binding is necessary to make `this` work in the callback
        this.handleClick = this.handleClick.bind(this);
    }

    handleClick() {
        const link = `https://localhost/fetch_raw?id=${this.props.id}`;
        console.log(link)
        fetch(link).then(async response => {
            const url = window.URL.createObjectURL(new Blob([await response.blob()]));
            const header = response.headers.get('Content-Disposition');
            const parts = header.split(';');
            const filename = parts[1].split('=')[1];
            const link = document.createElement("a");
            link.href = url;

            link.setAttribute('download', filename);
            document.body.appendChild(link);
            link.click();
            URL.revokeObjectURL(url);
            // Clean up and remove the link
            link.parentNode.removeChild(link);
        });
    }

    render() {
        return (
            <div>
                <div style= {{ marginBottom: "10px", display: "flex", justifyContent: "flex-end" }}><DownloadIcon height="25px" width="25px" onClick={this.handleClick}/></div>
                <img
                    width="100%"
                    alt="Search Result"
                    src={`https://localhost/fetch_jpg?id=${this.props.id}&width=1200&height=800&quality=75`}
                />
            </div>
        )
            ;
    }
}

export default ImageResult;