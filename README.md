# msuc-rs

![crates.io](https://img.shields.io/crates/v/msuc?link=https://crates.io/crates/msuc)

This crate provides a Rust client interface for the [Microsoft Update Catalog](https://www.catalog.update.microsoft.com/Home.aspx). It supports searching
the catalog and retrieve the details for specific updates.

## Documentation

See the [docs.rs](https://docs.rs/msuc) page for documentation.

## Usage

This crate is [on crates.io](https://crates.io/crates/msuc) and can be
used by adding `msuc` to your dependencies in your project's `Cargo.toml`.

```toml
[dependencies]
msuc = "1.0.0"
```

## Examples

### Searching the Update Catalog

```rust
use msuc::prelude::*;
#[tokio::main]
async fn main() {
    let client = MsucClient::new();
    let search = client.search("MS08-067");
    loop {
        match search.next().await {
            Ok(Some(results)) => {
                for r in results {
                    println!("title: {}", r.title);
                    println!("id: {}", r.id);
                    println!("kb: {}", r.kb);
                    println!("product: {}", r.product);
                    println!("classification: {}", r.classification);
                    println!("last modified: {}", r.last_modified);
                    println!("version: {}", r.version.unwrap_or("".to_string()));
                    println!("size: {}", r.size);
                    println!();
                }
            },
            Ok(None) => break,
            Err(e) => println!("error: {}", e),
        }
    }
}
```

### Retrieving the Details for an Update

```rust
use msuc::prelude::*;
#[tokio::main]
async fn main() {
    let client = MsucClient::new();
    // MS08-067: KB958644
    let details = client.details("9602ca4a-80a7-4d73-94c3-0088fcb5bce3").await;
    match details {
        Ok(d) => {
            println!("title: {}", d.title);
            println!("id: {}", d.id);
            println!("kb: {}", d.kb);
            println!("classification: {}", d.classification);
            println!("last modified: {}", d.last_modified);
            println!("size: {}", d.size);
            println!("description: {}", d.description);
            println!("architecture: {}", d.architecture);
            println!("supported products: {}", d.supported_products);
            println!("supported languages: {}", d.supported_languages);
            println!("msrc number: {}", d.msrc_number);
            println!("msrc severity: {}", d.msrc_severity);
            println!("info url: {}", d.info_url);
            println!("support url: {}", d.support_url);
            println!("reboot behavior: {}", d.reboot_behavior);
            println!("requires user input: {}", d.requires_user_input);
            println!("is exclusive install: {}", d.is_exclusive_install);
            println!("requires network connectivity: {}", d.requires_network_connectivity);
            println!("uninstall notes: {}", d.uninstall_notes);
            println!("uninstall steps: {}", d.uninstall_steps);
            println!("supersedes: {}", d.supersedes);
            println!("superseded by: {}", d.superseded_by);
        },
        Err(e) => println!("error: {}", e),
    }

}
```

## Crate features

The following crate features are available:

- default: async/await support
- blocking: blocking support

> **Note**: The `blocking` feature is mutually exclusive with the `default` feature.
