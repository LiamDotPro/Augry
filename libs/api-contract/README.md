# API contract

This folder is generated. Do not edit it by hand: changes are
overwritten by the next sync.

It holds the released contract of the Terminal Grid cloud API:

- `openapi.yaml` is the OpenAPI 3.0.3 description of the API.
- `fixtures/` holds example request and response bodies, one JSON file
  per case, named `<Schema>.<case>.json`. They are valid against the
  schemas in `openapi.yaml`, so client tests can use them as they are.

A pull request updates this folder after each release of the API that
changed the contract. Review it like any other change.
