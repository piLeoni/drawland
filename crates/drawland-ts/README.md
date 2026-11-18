# drawland-ts

TypeScript to JavaScript by taking the types out: every annotation,
interface and cast becomes blanks, so the JavaScript keeps every line and
column of the TypeScript, and an error in it points at the TypeScript as
written. How [draw.land](https://draw.land) runs `.ts` profiles. Only types:
an `enum`, a `namespace` or a parameter property is an error.

```rust
let js = drawland_ts::strip_types("const s: string = '0.5mm'")?;
assert_eq!(js, "const s         = '0.5mm'");
# Ok::<(), drawland_ts::Error>(())
```

swc's fast strip underneath (the one behind Node's `--experimental-strip-types`).
MIT.
