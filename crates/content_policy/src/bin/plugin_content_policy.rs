//! `plugin-content-policy`: the app's content rule for tools.
//!
//! Reads a JSON array of repository listings on stdin, each at least
//! `{"repositoryUrl": "...", "internalName": "...", "tvTypes": [...]}` (other fields are ignored),
//! and writes a JSON array with one verdict per listing, in the same order:
//! `{"repositoryUrl", "internalName", "video": bool, "reason": "..."}`.
//! Exit code 0 on success, 2 when the input is not such an array.

use std::io::{self, Read, Write};
use std::process::ExitCode;

use content_policy::{Allowed, ContentPolicy, PluginFacts, Verdict};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Listing {
    repository_url: String,
    internal_name: String,
    #[serde(default, deserialize_with = "nullable_list")]
    tv_types: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Decision<'a> {
    repository_url: &'a str,
    internal_name: &'a str,
    video: bool,
    reason: String,
}

/// Repositories write `"tvTypes": null` too (MegaProvider does).
fn nullable_list<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<String>, D::Error> {
    Ok(Option::<Vec<String>>::deserialize(deserializer)?.unwrap_or_default())
}

fn reason(verdict: &Verdict) -> String {
    match verdict {
        Verdict::Video(Allowed::ByTypes(types)) => {
            let names: Vec<&str> = types.iter().map(|media| media.name()).collect();
            format!("video ({})", names.join(", "))
        }
        Verdict::Video(Allowed::Reviewed(why)) => format!("reviewed: {why}"),
        Verdict::NotVideo(refusal) => refusal.to_string(),
    }
}

fn decide(listings: &[Listing]) -> Vec<Decision<'_>> {
    let policy = ContentPolicy::standard();
    listings
        .iter()
        .map(|listing| {
            let verdict = policy.judge(&PluginFacts {
                repository_url: &listing.repository_url,
                internal_name: &listing.internal_name,
                declared_types: &listing.tv_types,
            });
            Decision {
                repository_url: &listing.repository_url,
                internal_name: &listing.internal_name,
                video: verdict.is_video(),
                reason: reason(&verdict),
            }
        })
        .collect()
}

fn run() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| format!("cannot read stdin: {error}"))?;
    let listings: Vec<Listing> = serde_json::from_str(&input)
        .map_err(|error| format!("input is not a listing array: {error}"))?;
    let output = serde_json::to_string(&decide(&listings))
        .map_err(|error| format!("cannot write the verdicts: {error}"))?;
    writeln!(io::stdout().lock(), "{output}")
        .map_err(|error| format!("cannot write stdout: {error}"))
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            let _ = writeln!(io::stderr().lock(), "plugin-content-policy: {message}");
            ExitCode::from(2)
        }
    }
}
