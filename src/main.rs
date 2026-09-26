//! sherlock-rs — hunt a username across social networks.
//!
//! Concurrent Rust rewrite of sherlock-project/sherlock (MIT). Given one or more
//! usernames, it queries each site in the catalog in parallel and reports where
//! the name appears to be registered.
//!
//! OSINT tool for authorized research and footprinting against public web
//! endpoints. Respect each site's terms of service and applicable law.

use std::time::Duration;

use clap::Parser;
use futures::stream::{self, StreamExt};

mod sites;
use sites::{ErrorType, Site};

#[derive(Parser, Debug)]
#[command(name = "sherlock-rs", version, about = "Hunt usernames across social networks")]
struct Cli {
    /// One or more usernames to search for.
    #[arg(required = true)]
    usernames: Vec<String>,

    /// Path to the site catalog JSON.
    #[arg(short, long, default_value = "sites.json")]
    sites: String,

    /// Max concurrent requests.
    #[arg(short, long, default_value_t = 20)]
    concurrency: usize,

    /// Per-request timeout, in seconds.
    #[arg(short, long, default_value_t = 10)]
    timeout: u64,

    /// Only print sites where the username was found.
    #[arg(long)]
    found_only: bool,
}

#[derive(Debug)]
enum Outcome {
    Found(String),   // url
    NotFound,
    Error(String),
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let catalog = match sites::load(&cli.sites) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: could not load site catalog: {e}");
            std::process::exit(2);
        }
    };

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(cli.timeout))
        .user_agent("sherlock-rs/0.1 (+https://github.com/cloudsculptinglabs/sherlock-rs)")
        .build()
        .expect("failed to build HTTP client");

    for username in &cli.usernames {
        println!("\n[*] Checking username: {username}");

        let checks = catalog.iter().map(|(name, site)| {
            let client = client.clone();
            let username = username.clone();
            let name = name.clone();
            let site = site.clone();
            async move {
                let outcome = check(&client, &site, &username).await;
                (name, outcome)
            }
        });

        let mut results: Vec<(String, Outcome)> = stream::iter(checks)
            .buffer_unordered(cli.concurrency)
            .collect()
            .await;
        results.sort_by(|a, b| a.0.cmp(&b.0));

        for (name, outcome) in results {
            match outcome {
                Outcome::Found(url) => println!("[+] {name}: {url}"),
                Outcome::NotFound if !cli.found_only => println!("[-] {name}: not found"),
                Outcome::Error(e) if !cli.found_only => println!("[!] {name}: {e}"),
                _ => {}
            }
        }
    }
}

async fn check(client: &reqwest::Client, site: &Site, username: &str) -> Outcome {
    let url = site.url.replace("{}", username);
    let resp = match client.get(&url).send().await {
        Ok(r) => r,
        Err(e) => return Outcome::Error(e.to_string()),
    };

    match site.error_type {
        ErrorType::StatusCode => {
            if resp.status().is_success() {
                Outcome::Found(url)
            } else {
                Outcome::NotFound
            }
        }
        ErrorType::Message => {
            let marker = site.error_msg.clone().unwrap_or_default();
            match resp.text().await {
                Ok(body) if !marker.is_empty() && body.contains(&marker) => Outcome::NotFound,
                Ok(_) => Outcome::Found(url),
                Err(e) => Outcome::Error(e.to_string()),
            }
        }
    }
}
