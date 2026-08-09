<div align="center">

# feed

![version](https://img.shields.io/badge/version-0.1.1-blue) ![crate](https://img.shields.io/badge/crate-fe2o3--feed-orange) ![license](https://img.shields.io/badge/license-Unlicense-green) ![Stay Amazing](https://img.shields.io/badge/Stay-Amazing-important)

RSS and Atom parsing for the [Fe₂O₃](https://github.com/isene/fe2o3) suite.

</div>

Give it the XML, get back the entries. Fetching is the caller's business:
[kastrup](https://github.com/isene/kastrup) shells out to curl, the
[nomad](https://github.com/isene/nomad) phone app uses its own HTTP
client, and neither belongs in here.

```rust
for item in feed::parse(&xml, "isene.org", url) {
    println!("{} — {}", item.title, item.link);
}
```

## Not an XML parser

Deliberately. Feeds in the wild are ill-formed often enough that a strict
parser is a liability, and the shape being read is shallow: find a tag,
take what is between it and its close. CDATA and the five XML entities
are handled; namespaces are ignored, which is exactly why `dc:creator`
and `content:encoded` work by name.

Dates arrive in two spellings — RFC 2822 (`Thu, 3 Apr 2026 09:15:00
+0200`) and ISO 8601 (`2026-04-03T09:15:00Z`) — and both are read, with
their zone offsets, into Unix seconds. A feed that gives no usable date
yields 0 rather than today, so a sort never lies about it.

## Identity

An item's id is its own `guid`/`id` if it has one, else its link, else
its title. Feeds reorder and repaginate constantly, so position means
nothing, and an id that changed with the link would orphan the entry
every time a site tidied its URLs.

## License

Public domain. See [Unlicense](LICENSE).
