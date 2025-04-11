-- Add migration script here

CREATE TABLE offers (
    code BINARY(12) NOT NULL PRIMARY KEY,
    offer BLOB NOT NULL
);
