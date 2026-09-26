# sherlock-rs

Hunt a username across social networks. Give it one or more usernames and it
queries every site in its catalog **in parallel**, reporting where each name
appears to be registered.

A fast, single-binary Rust rewrite of
[sherlock-project/sherlock](https://github.com/sherlock-project/sherlock)
(Python, MIT). No interpreter, no `pip install` on the box — async requests via
`tokio` + `reqwest`, with a JSON site catalog you can extend.

> **Scope & ethics.** An OSINT tool for authorized research and footprinting
> against public web endpoints. Respect each site's terms of service, rate
> limits, and the privacy of the people behind the accounts. Use it lawfully.

## Status

Early scaffold — concurrent checking, status-code and body-message detection,
and a starter catalog of 8 sites. The upstream catalog has 400+; expanding
`sites.json` is the main next step.

## Build

```sh
cargo build --release
# binary at target/release/sherlock-rs
```

## Usage

```sh
sherlock-rs alice                      # check one username
sherlock-rs alice bob carol            # check several
sherlock-rs --found-only alice         # only print hits
sherlock-rs -c 50 -t 5 alice           # 50 concurrent, 5s timeout
sherlock-rs -s my-sites.json alice     # use a custom catalog
```

## Site catalog

`sites.json` maps a site name to a check:

```json
{
  "GitHub": { "url": "https://github.com/{}", "errorType": "status_code" },
  "Steam":  { "url": "https://steamcommunity.com/id/{}", "errorType": "message",
              "errorMsg": "The specified profile could not be found" }
}
```

- `status_code` — the username exists if the profile URL returns HTTP 2xx.
- `message` — it exists if `errorMsg` is **absent** from the response body.

## License

MIT, matching the original Sherlock. See [`LICENSE`](./LICENSE).

## Roadmap

- [ ] Import the full upstream site catalog (400+ sites)
- [ ] `response_url` (redirect) detection type
- [ ] Per-site request headers and rate limiting
- [ ] CSV / JSON output for pipelines
- [ ] Proxy and Tor support
