# msuc-rs

![crates.io](https://img.shields.io/crates/v/msuc?link=https://crates.io/crates/msuc)

A rust client for the [Microsoft Update Catalog](https://www.catalog.update.microsoft.com/Home.aspx).

## Documentation

See the [docs.rs](https://docs.rs/msuc) page for documentation.

## TODO

- [ ] Add support for the `tracing` crate
- [ ] Consider returning an iterator for search results
- [ ] Add page count to search meta
  - Just need to parse this from the HTML
- [ ] Inform user when search results are truncated by the Microsoft Update Catalog (1000+ results)
  - Today this state is stored internally, but not exposed to the user