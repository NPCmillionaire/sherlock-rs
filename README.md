# sherlock-rs

Hunt a username across the web. Give it one or more usernames and it queries
every site in its catalog **in parallel**, reporting where each name appears to
be registered.

A fast, single-binary Rust rewrite of
[sherlock-project/sherlock](https://github.com/sherlock-project/sherlock)
(Python, MIT) — with an accuracy-first detection engine, structured output, and
a categorized multi-source catalog.

> **Scope & ethics.** An OSINT tool for authorized research and footprinting
> against public web endpoints. It checks **presence only** — it does not scrape
> profile contents. Respect each site's rate limits and applicable law.

## What's new in 0.2

- **Baseline probing** — for every site it also queries a known-absent (random)
  username and only reports a hit when the target's response genuinely differs.
  This kills the false positives that plague naive status-code checks.
- **Three detection types** — `status_code`, `response_url` (redirect-aware),
  and `message` (regex body markers).
- **Robustness** — retries with exponential backoff, a distinct `blocked` state
  for 429/403 (instead of silent false-negatives), `--proxy` (http/https/socks5),
  and a `--delay` throttle.
- **Structured output** — `--format text|json|csv` for pipelines.
- **Filters** — `--site`, `--exclude`, and `--category`.
- **Bigger catalog** — 38 sites across dev, social, gaming, video, music, and
  creative, embedded in the binary (override with `--sites`).

## Build

```sh
cargo build --release
# binary at target/release/sherlock-rs
```

## Usage

```sh
sherlock-rs alice                          # check one username
sherlock-rs alice bob carol                # check several
sherlock-rs --found-only alice             # only print hits
sherlock-rs --category dev alice           # only dev-platform sites
sherlock-rs --site GitHub --site Reddit x  # just these sites
sherlock-rs --format json alice > out.json # machine-readable
sherlock-rs --proxy socks5://127.0.0.1:9050 alice   # over Tor
sherlock-rs --fast alice                   # skip baseline probing (faster, less accurate)
```

## Detection types

Each site in `sites.json` declares how to tell a hit from a miss:

- `status_code` — exists if the profile URL returns 2xx **and** the known-absent
  baseline does not.
- `response_url` — exists if the target URL stays put while the baseline gets
  redirected to a login / not-found page.
- `message` — exists if a regex marker of "not found" is absent from the body
  (trusted only when the baseline confirms the marker appears for absent users).

```json
{
  "GitHub":   { "url": "https://github.com/{}", "errorType": "status_code", "category": "dev" },
  "Pinterest":{ "url": "https://www.pinterest.com/{}/", "errorType": "response_url", "category": "social" },
  "Steam":    { "url": "https://steamcommunity.com/id/{}", "errorType": "message",
                "errorMsg": "The specified profile could not be found", "category": "gaming" }
}
```

## License

MIT, matching the original Sherlock. See [`LICENSE`](./LICENSE).

## Roadmap

- [ ] Import the full upstream catalog (400+ sites)
- [ ] Per-site request headers, methods (POST), and NSFW tagging
- [ ] Other source *types* as modules: email/breach lookups, domain WHOIS/DNS
- [ ] Rate-limit governor and adaptive concurrency
- [ ] Tune known-loose sites (e.g. Medium returns 200 for any handle)
