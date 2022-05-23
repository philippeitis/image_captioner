import React from 'react';
import downloadIcon from "./save-file.svg";

class ImageResult extends React.Component {
    constructor(props) {
        super(props);
        console.log(props)
        this.state = {id: props.data._additional.id, image: props.data.image};

        // This binding is necessary to make `this` work in the callback
        this.handleClick = this.handleClick.bind(this);
    }

    handleClick() {
        const link = `http://localhost:8081/fetch_raw?id=${this.state.id}`;
        console.log(link);
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

            // Clean up and remove the link
            link.parentNode.removeChild(link);
        });
    }

    render() {
        return (
            <div>
                <button onClick={this.handleClick}><img src={downloadIcon.default} alt="Download"/></button>
                <img
                    width="100%"
                    alt="Search Result"
                    src={
                        'data:image/jpg;base64,' +
                        this.state.image
                    }
                />
            </div>
        )
            ;
    }
}

export default ImageResult;