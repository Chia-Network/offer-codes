# Offer Codes

This is a very simple and lightweight API for uploading and downloading offer files with short codes. It uses a BLS public key to authenticate upload requests, and there is no frontend at this time.

## Setup

1. Install Rust and MySQL (or MariaDB)
2. Copy `example.env` to `.env` and fill in the appropriate values
3. Run `cargo run` to start the API
4. You can use `index.js` as an example for how to connect to it as a client to sign and submit an upload request, and then later download the offer by its short code

The API always connects to MySQL over TLS, so the server must have TLS enabled. MySQL 8 enables it by default with generated certificates, while MariaDB needs `ssl_cert` and `ssl_key` configured. Set `MYSQL_SSL_CA` to a CA file to verify the server certificate and hostname.
