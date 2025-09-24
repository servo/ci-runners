use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader},
};

use chrono::{DateTime, TimeDelta};
use clap::Parser;
use jane_eyre::eyre;
use regex::Regex;

#[derive(clap::Parser)]
struct Args {
    logs: Vec<String>,
}

fn main() -> eyre::Result<()> {
    let mut monthly_usage_by_profile: BTreeMap<String, TimeDelta> = BTreeMap::default();
    for log in Args::parse().logs {
        println!("### {log}");
        let file = File::open(log)?;
        let result = analyse(BufReader::new(file))?;
        for (profile, usage) in result.usage_by_profile {
            let monthly_scale_factor =
                TimeDelta::days(30).as_seconds_f64() / result.server_uptime.as_seconds_f64();
            let monthly_usage = usage.as_seconds_f64() * monthly_scale_factor;
            let monthly_usage = TimeDelta::nanoseconds((monthly_usage * 1_000_000_000.0) as _);
            *monthly_usage_by_profile.entry(profile).or_default() += monthly_usage;
        }
    }

    println!("### Monthly usage (per month of 30 days)");
    println!("Runner hours spent in Busy, scaled to 30 days:");
    for (profile, usage) in &monthly_usage_by_profile {
        println!("- {profile}: {usage} ({})", day_string(*usage));
    }

    println!("### Equivalent spend (per month of 30 days)");
    println!("NOTE: this doesn’t even consider the speedup vs free runners!");
    // 1 EUR = 1.1799 USD (2025-09-24), so 1 USD/min = 1220.45 EUR/day
    let usd_per_eur = 1.1799;
    let min_per_day = 1440.0;
    let s_per_day = 86400.0;
    let competitor_pricing = BTreeMap::from([
        // As of 2025-09-24
        // <https://docs.github.com/en/billing/reference/actions-minute-multipliers>
        // <https://namespace.so/pricing>
        // <https://www.warpbuild.com/pricing>
        (
            "servo-macos13",
            sort_competitors([
                ("GitHub macOS arm64 5cpu", 0.16 / usd_per_eur * min_per_day),
                (
                    "Namespace macOS arm64 5cpu",
                    5.0 * 10.0 * 0.0015 / usd_per_eur * min_per_day,
                ),
                (
                    "WarpBuild macOS arm64 6cpu",
                    0.08 / usd_per_eur * min_per_day,
                ),
            ]),
        ),
        (
            "servo-ubuntu2204",
            sort_competitors([
                ("GitHub Linux x64 8cpu", 0.032 / usd_per_eur * min_per_day),
                (
                    "Namespace Linux x64 8cpu",
                    8.0 * 1.0 * 0.0015 / usd_per_eur * min_per_day,
                ),
                (
                    "WarpBuild Linux x64 8cpu",
                    0.016 / usd_per_eur * min_per_day,
                ),
                (
                    "WarpBuild Linux arm64 8cpu",
                    0.012 / usd_per_eur * min_per_day,
                ),
            ]),
        ),
        (
            "servo-windows10",
            sort_competitors([
                ("GitHub Windows x64 8cpu", 0.064 / usd_per_eur * min_per_day),
                (
                    "Namespace Windows x64 8cpu",
                    8.0 * 2.0 * 0.0015 / usd_per_eur * min_per_day,
                ),
                (
                    "WarpBuild Linux x64 8cpu",
                    0.032 / usd_per_eur * min_per_day,
                ),
            ]),
        ),
    ]);
    for (profile, competitors) in competitor_pricing {
        println!("- {profile}:");
        for (competitor_name, eur_per_day) in competitors {
            if let Some(usage) = monthly_usage_by_profile.get(profile) {
                let usage_s = usage.as_seconds_f64();
                let monthly_spend = usage_s / s_per_day * eur_per_day;
                println!("    - {competitor_name}:");
                println!(
                    "      {}/month × {eur_per_day:.2} EUR/day = {monthly_spend:.2} EUR/month",
                    day_string(*usage)
                );
            }
        }
    }

    Ok(())
}

fn day_string(duration: TimeDelta) -> String {
    format!("{:.2} days", duration.as_seconds_f64() / 86400.0)
}

struct AnalyseResult {
    usage_by_profile: BTreeMap<String, TimeDelta>,
    server_uptime: TimeDelta,
}

fn analyse(log: impl BufRead) -> Result<AnalyseResult, eyre::Error> {
    let header = Regex::new(r"^monitor: [^ ]+ registrations, [^ ]+ guests$")?;
    let runner =
        Regex::new(r"^monitor::runner: \[([0-9]+)\] profile ([^,]+), [^,]+, status ([^,]+)")?;
    let mut server_uptime = TimeDelta::zero();
    let mut last_header: Option<DateTime<_>> = None;
    let mut pending_runner_statuses = vec![];
    let mut runners: BTreeMap<(String, String, String), TimeDelta> = BTreeMap::default();
    for (i, line) in log.lines().enumerate() {
        if i % 100 == 0 {
            eprint!("\r{i}");
        }
        let line = line?;
        // Counterexample: `-- Boot 53133c5ab29a4295a65cb6bfde81cccd --`
        let Some((_prefix, rest)) = line.split_once(": ") else {
            continue;
        };
        // Counterexample: `Aug 06 06:52:23 ci0 monitor-start[2862247]: {"total_count":6,"labels":...`
        let Some((timestamp, rest)) = rest.split_once(" ") else {
            continue;
        };
        // Counterexample: `Aug 06 07:00:48 ci0 monitor-start[2870589]: Domain 'ci-servo-windows10.52349' started`
        let Ok(timestamp) = DateTime::parse_from_rfc3339(timestamp) else {
            continue;
        };
        // Counterexample: `Aug 06 06:55:27 ci0 monitor-start[2826158]: 2025-08-06T06:55:27.081960Z  WARN monitor::_: Request guard `ApiKeyGuard` failed: ().`
        let Some(message) = rest.trim_ascii_start().strip_prefix("INFO ") else {
            continue;
        };
        if header.is_match(message) {
            if let Some(last_header) = last_header {
                // Record against each `runner` the time elapsed by the surrounding `header` lines.
                // Status updates should happen every 5 s. If this update took more than 10 s,
                // the monitor might have died. The longer the monitor is down for, the more likely
                // it is that there are no healthy runners, so let’s assume that this is the case.
                let elapsed = (timestamp - last_header).min(TimeDelta::seconds(10));
                for (id, profile, status) in pending_runner_statuses.drain(..) {
                    *runners.entry((id, profile, status)).or_default() += elapsed;
                }
                server_uptime += elapsed;
            }
            last_header = Some(timestamp);
        } else if let Some(runner) = runner.captures(message) {
            let id = runner
                .get(1)
                .expect("Guaranteed by regex")
                .as_str()
                .to_owned();
            let profile = runner
                .get(2)
                .expect("Guaranteed by regex")
                .as_str()
                .to_owned();
            let status = runner
                .get(3)
                .expect("Guaranteed by regex")
                .as_str()
                .to_owned();
            pending_runner_statuses.push((id, profile, status));
        }
    }
    eprint!("\r"); // eprintln!();
    let mut runners_by_profile: BTreeMap<&String, Vec<(&String, &String, &TimeDelta)>> =
        BTreeMap::default();
    for ((id, profile, status), duration) in &runners {
        runners_by_profile
            .entry(profile)
            .or_default()
            .push((id, status, duration));
    }

    let mut usage_by_profile: BTreeMap<&String, TimeDelta> = BTreeMap::default();
    println!(
        "Over the last {server_uptime} ({}) of uptime:",
        day_string(server_uptime)
    );
    for (profile, runners) in &runners_by_profile {
        let mut runner_ids = BTreeSet::default();
        let mut durations_by_status: BTreeMap<&String, TimeDelta> = BTreeMap::default();
        for (id, status, duration) in runners {
            runner_ids.insert(id);
            *durations_by_status.entry(status).or_default() += **duration;
            if *status == "Busy" {
                *usage_by_profile.entry(profile).or_default() += **duration;
            }
        }
        println!("- {} runners in profile {profile}:", runner_ids.len());
        for (status, duration) in durations_by_status {
            let duty_percent = 100.0 * duration.as_seconds_f64() / server_uptime.as_seconds_f64();
            println!(
                "    - {status} for {duty_percent:.2}%, {duration} ({})",
                day_string(duration)
            );
        }
    }

    Ok(AnalyseResult {
        usage_by_profile: usage_by_profile
            .into_iter()
            .map(|(profile, usage)| (profile.to_owned(), usage))
            .collect(),
        server_uptime,
    })
}

fn sort_competitors<const N: usize>(competitors: [(&str, f64); N]) -> Vec<(&str, f64)> {
    let mut result = competitors.to_vec();
    result.sort_by(|(_, p), (_, q)| p.total_cmp(q));
    result
}
