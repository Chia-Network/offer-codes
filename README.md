# Offer Codes

This is a very simple and lightweight API for uploading and downloading offer files with short codes. It uses a BLS public key to authenticate upload requests, and there is no frontend at this time.

## Setup

1. Install Rust and MySQL (or MariaDB)
2. Copy `example.env` to `.env` and fill in the appropriate values
3. Run `cargo run` to start the API
4. You can use `index.js` as an example for how to connect to it as a client to sign and submit an upload request, and then later download the offer by its short code
