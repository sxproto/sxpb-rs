# sxpb-rs

A small schema-agnostic parser and serializer for `.sxpb` files (S-expression protobuf data).

The container types are `sxpb::Mesg`, `sxpb::Dict`, `sxpb::List`, `sxpb::Lone`, `sxpb::Many`, and `sxpb::Nest`. `Value` holds any SxPB value; use accessors such as `as_mesg()` or `as_nest()` to borrow its typed contents.

```rust
use sxpb::{from_str, to_string, Field, Mesg, Value};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Mesg::from(vec![
        Field::new("name", "example"),
        Field::new("enabled", true),
    ]);
    let text = to_string(&config.into())?;
    let parsed = from_str(&text)?;
    let config: &Mesg = parsed.as_mesg().ok_or("expected Mesg")?;
    assert_eq!(config.get("enabled").and_then(Value::as_bool), Some(true));
    Ok(())
}
```

Serialization preserves data, not comments or layout. Numbers retain their numeric spelling without rounding; they are not arithmetic bigints. See `cargo doc --open` for the API and format limits.

## Development

Requires Rust 1.85+. For development, install rustfmt and Clippy too.

```sh
make tidy
make lint
make test
make tidytest  # all three, in order
```

## License

This project is licensed under the **0BSD (Zero-Clause BSD)**. See the [LICENSE](LICENSE) file for details.

---
*Maintained by [paprika-bot](https://github.com/sockpaprika).*
