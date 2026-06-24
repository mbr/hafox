# hafox

`hafox` reads the local SmartFox `values.xml` endpoint and prints a normalized Rust domain model.

## Usage

```sh
hafox fetch --smartfox-url http://smartfox
```

The command fetches current measurements, parses the XML payload, derives the live power-flow values, and prints the snapshot with Rust debug formatting.
