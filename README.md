SCAMP - Rust edition
=====================

The `scamp` crate provides all the facilities necessary for participating in a SCAMP microservice environment:

  * Parsing the discovery cache and building a directory of available services
  * Parsing packets streams
  * Parsing and verifying messages

Architecture
--------

Remote services are invoked by first establishing a `Connection` (TLS under the hood). This `Connection` can then be used to spawn `Sessions` which send a `Request` and block until a `Reply` is provided.

Usage
-----

TBD
