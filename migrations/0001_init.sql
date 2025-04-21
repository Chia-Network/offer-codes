-- Add migration script here

CREATE TABLE offers (
    code BINARY(32) NOT NULL PRIMARY KEY,
    offer BLOB NOT NULL
);
