//! sherlock-rs — hunt a username across the web.
//!
//! Concurrent Rust rewrite of sherlock-project/sherlock (MIT). For each username
//! it queries every site in the catalog in parallel and reports where the name
//! appears to be registered.
//!
//! Accuracy first: for each site it also probes a known-absent (random) username
//! to establish a baseline, and only reports a hit when the target's response
//! genuinely differs from that baseline. This suppresses the false positives that
//! plague naive status-code checks.
//!
//! OSINT tool for authorized research and footprinting against public web
//! endpoints. Checks presence only — it does not scrape profile contents.

use std::time::Duration;

use clap::Parser;
use futures::stream::{self, StreamExt};
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use regex::Regex;
use serde::Serialize;

mod sites;
use sites::{ErrorType, Site};

#[derive(Parser, Debug)]
#[command(name = "sherlock-rs", version, about = "Hunt usernames across the web")]
struct Cli {
    /// One or more usernames to search for.
    #[arg(required = true)]
    usernames: Vec<String>,

    /// Path to a site catalog JSON (overrides the built-in catalog).
    #[arg(short, long)]
    sites: Option<String>,

    /// Only check these sites (by name, case-insensitive). Repeatable.
    #[arg(long = "site")]
    only: Vec<String>,

    /// Skip these sites (by name, case-insensitive). Repeatable.
    #[arg(long)]
    exclude: Vec<String>,

    /// Only check sites in this category (social, dev, gaming, ...). Repeatable.
    #[arg(long)]
    category: Vec<String>,

    /// Max concurrent requests.
    #[arg(short, long, default_value_t = 20)]
    concurrency: usize,

    /// Per-request timeout, in seconds.
    #[arg(short, long, default_value_t = 10)]
    timeout: u64,

    /// Delay before each request, in milliseconds (politeness / anti-ban).
    #[arg(long, default_value_t = 0)]
    delay: u64,

    /// Retries on 429 / 5xx / transport errors.
    #[arg(long, default_value_t = 2)]
    retries: u32,

    /// Route requests through a proxy (http/https/socks5), e.g. socks5://127.0.0.1:9050.
    #[arg(long)]
    proxy: Option<String>,

    /// Skip baseline probing (faster, less accurate).
    #[arg(long)]
    fast: bool,

    /// Only print sites where the username was found.
    #[arg(long)]
    found_only: bool,

    /// Output format: text | json | csv.
    #[arg(long, default_value = "text")]
    format: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
enum Status {
    Found { url: String },
    NotFound,
    Blocked,
    Skipped { reason: String },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize)]
struct Finding {
    username: String,
    site: String,
    category: Option<String>,
    #[serde(flatten)]
    status: Status,
}

struct Probe {
    status: u16,
    final_url: String,
    body: Option<String>,
    err: Option<String>,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let mut catalog = match &cli.sites {
        Some(path) => match sites::load_from_file(path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("error: could not load site catalog: {e}");
                std::process::exit(2);
            }
        },
        None => sites::builtin(),
    };

    // Name / category filters.
    let only: Vec<String> = cli.only.iter().map(|s| s.to_lowercase()).collect();
    let exclude: Vec<String> = cli.exclude.iter().map(|s| s.to_lowercase()).collect();
    let cats: Vec<String> = cli.category.iter().map(|s| s.to_lowercase()).collect();
    catalog.retain(|name, site| {
        let n = name.to_lowercase();
        if !only.is_empty() && !only.contains(&n) {
            return false;
        }
        if exclude.contains(&n) {
            return false;
        }
        if !cats.is_empty() {
            let c = site.category.clone().unwrap_or_default().to_lowercase();
            if !cats.contains(&c) {
                return false;
            }
        }
        true
    });

    if catalog.is_empty() {
        eprintln!("error: no sites left after filters");
        std::process::exit(2);
    }

    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(cli.timeout))
        .user_agent("Mozilla/5.0 (compatible; sherlock-rs/0.2; +https://github.com/cloudsculptinglabs/sherlock-rs)");
    if let Some(p) = &cli.proxy {
        match reqwest::Proxy::all(p) {
            Ok(proxy) => builder = builder.proxy(proxy),
            Err(e) => {
                eprintln!("error: invalid proxy {p}: {e}");
                std::process::exit(2);
            }
        }
    }
    let client = builder.build().expect("failed to build HTTP client");

    let text_out = cli.format == "text";
    let mut all: Vec<Finding> = Vec::new();

    for username in &cli.usernames {
        if text_out {
            println!("\n[*] Checking username: {username}");
        }

        let baseline_user = random_username();
        let pb = if text_out {
            let pb = ProgressBar::new(catalog.len() as u64);
            pb.set_style(
                ProgressStyle::with_template("  [{bar:30}] {pos}/{len} {msg}")
                    .unwrap()
                    .progress_chars("=> "),
            );
            Some(pb)
        } else {
            None
        };

        let checks = catalog.iter().map(|(name, site)| {
            let client = client.clone();
            let username = username.clone();
            let baseline_user = baseline_user.clone();
            let name = name.clone();
            let site = site.clone();
            let delay = cli.delay;
            let retries = cli.retries;
            let fast = cli.fast;
            async move {
                let status =
                    check(&client, &site, &username, &baseline_user, delay, retries, fast).await;
                Finding {
                    username,
                    site: name,
                    category: site.category.clone(),
                    status,
                }
            }
        });

        let mut results: Vec<Finding> = stream::iter(checks)
            .buffer_unordered(cli.concurrency)
            .map(|f| {
                if let Some(pb) = &pb {
                    pb.inc(1);
                }
                f
            })
            .collect()
            .await;
        if let Some(pb) = &pb {
            pb.finish_and_clear();
        }
        results.sort_by(|a, b| a.site.cmp(&b.site));

        if text_out {
            for f in &results {
                match &f.status {
                    Status::Found { url } => println!("[+] {}: {url}", f.site),
                    Status::Blocked if !cli.found_only => {
                        println!("[!] {}: blocked / rate-limited", f.site)
                    }
                    Status::Error { message } if !cli.found_only => {
                        println!("[!] {}: {message}", f.site)
                    }
                    Status::Skipped { reason } if !cli.found_only => {
                        println!("[.] {}: skipped ({reason})", f.site)
                    }
                    Status::NotFound if !cli.found_only => println!("[-] {}: not found", f.site),
                    _ => {}
                }
            }
            let found = results
                .iter()
                .filter(|f| matches!(f.status, Status::Found { .. }))
                .count();
            println!("[*] {username}: {found} found across {} sites", results.len());
        }

        all.extend(results);
    }

    match cli.format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&all).unwrap()),
        "csv" => print_csv(&all),
        "text" => {}
        other => eprintln!("warning: unknown format '{other}', defaulted to text"),
    }
}

/// Perform baseline + target probes and decide the outcome.
async fn check(
    client: &reqwest::Client,
    site: &Site,
    username: &str,
    baseline_user: &str,
    delay: u64,
    retries: u32,
    fast: bool,
) -> Status {
    // Username must satisfy the site's constraints, if any.
    if let Some(rx) = &site.regex_check {
        match Regex::new(rx) {
            Ok(re) if !re.is_match(username) => {
                return Status::Skipped {
                    reason: "username fails site format".into(),
                }
            }
            Err(_) => {} // bad regex in catalog: ignore the constraint
            _ => {}
        }
    }

    let need_body = site.error_type == ErrorType::Message;
    let target_url = site.url.replace("{}", username);
    let target = fetch(client, &target_url, need_body, delay, retries).await;

    if let Some(e) = &target.err {
        return Status::Error { message: e.clone() };
    }
    if target.status == 429 || target.status == 403 {
        return Status::Blocked;
    }

    // Baseline probe against a username that cannot exist.
    let baseline = if fast {
        None
    } else {
        let burl = site.url.replace("{}", baseline_user);
        Some(fetch(client, &burl, need_body, delay, retries).await)
    };

    match site.error_type {
        ErrorType::StatusCode => {
            let target_ok = (200..300).contains(&target.status);
            match &baseline {
                // If the known-absent user also returns 2xx, status code is
                // meaningless for this site — don't claim a hit.
                Some(b) if (200..300).contains(&b.status) => Status::NotFound,
                _ if target_ok => Status::Found { url: target_url },
                _ => Status::NotFound,
            }
        }
        ErrorType::ResponseUrl => {
            let target_stayed = urls_match(&target_url, &target.final_url);
            match &baseline {
                Some(b) => {
                    let base_url = site.url.replace("{}", baseline_user);
                    let baseline_stayed = urls_match(&base_url, &b.final_url);
                    // Hit only if the target stayed put AND the absent user got
                    // redirected away (proving redirect is the "not found" tell).
                    if target_stayed && !baseline_stayed {
                        Status::Found { url: target_url }
                    } else {
                        Status::NotFound
                    }
                }
                None if target_stayed => Status::Found { url: target_url },
                None => Status::NotFound,
            }
        }
        ErrorType::Message => {
            let marker = site.error_msg.clone().unwrap_or_default();
            let re = Regex::new(&regex::escape(&marker)).ok();
            let contains = |body: &Option<String>| -> bool {
                match (body, &re) {
                    (Some(b), Some(re)) if !marker.is_empty() => re.is_match(b),
                    _ => false,
                }
            };
            let target_absent = contains(&target.body); // marker present => not found
            match &baseline {
                // Trust the marker only if it actually shows up for a known-absent
                // user. Otherwise the rule is wrong; stay conservative.
                Some(b) if !contains(&b.body) => Status::NotFound,
                _ if target_absent => Status::NotFound,
                _ => Status::Found { url: target_url },
            }
        }
    }
}

/// GET a URL with retries + backoff on 429/5xx/transport errors.
async fn fetch(
    client: &reqwest::Client,
    url: &str,
    need_body: bool,
    delay: u64,
    retries: u32,
) -> Probe {
    let mut attempt = 0;
    loop {
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        match client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let final_url = resp.url().to_string();
                if (status == 429 || status >= 500) && attempt < retries {
                    backoff(attempt).await;
                    attempt += 1;
                    continue;
                }
                let body = if need_body {
                    resp.text().await.ok()
                } else {
                    None
                };
                return Probe {
                    status,
                    final_url,
                    body,
                    err: None,
                };
            }
            Err(e) => {
                if attempt < retries {
                    backoff(attempt).await;
                    attempt += 1;
                    continue;
                }
                return Probe {
                    status: 0,
                    final_url: url.to_string(),
                    body: None,
                    err: Some(e.to_string()),
                };
            }
        }
    }
}

async fn backoff(attempt: u32) {
    let ms = 200u64 * 2u64.pow(attempt);
    tokio::time::sleep(Duration::from_millis(ms)).await;
}

/// Compare two URLs ignoring trailing slash and scheme differences.
fn urls_match(a: &str, b: &str) -> bool {
    let norm = |u: &str| {
        u.trim_end_matches('/')
            .replace("https://", "")
            .replace("http://", "")
            .to_lowercase()
    };
    norm(a) == norm(b)
}

fn random_username() -> String {
    let mut rng = rand::thread_rng();
    let suffix: String = (0..12)
        .map(|_| {
            let c = rng.gen_range(0..36);
            if c < 10 {
                (b'0' + c) as char
            } else {
                (b'a' + (c - 10)) as char
            }
        })
        .collect();
    format!("zz{suffix}")
}

fn print_csv(findings: &[Finding]) {
    println!("username,site,category,status,url");
    for f in findings {
        let (status, url) = match &f.status {
            Status::Found { url } => ("found", url.clone()),
            Status::NotFound => ("not_found", String::new()),
            Status::Blocked => ("blocked", String::new()),
            Status::Skipped { .. } => ("skipped", String::new()),
            Status::Error { .. } => ("error", String::new()),
        };
        let cat = f.category.clone().unwrap_or_default();
        println!(
            "{},{},{},{},{}",
            csv_field(&f.username),
            csv_field(&f.site),
            csv_field(&cat),
            status,
            csv_field(&url)
        );
    }
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}
