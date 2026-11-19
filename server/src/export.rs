use std::io::Write;

use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, DbConn, EntityTrait, QueryFilter};
use zip::{ZipWriter, write::SimpleFileOptions, CompressionMethod};

use crate::ballots;
use crate::ballot_tokens;
use crate::counting;
use crate::elections;
use crate::error::Error;
use crate::log::{self, CountingLogActionType, CountingLogCandidateStatus};

pub async fn build_export_zip(
    db: &DbConn,
    election_uuid: &str,
    include_emails: bool,
) -> Result<Vec<u8>, Error> {
    let election = elections::entity::Entity::find_by_id(election_uuid)
        .one(db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query election: {}", e)))?
        .ok_or(Error::NotFound)?;

    let all_tokens = ballot_tokens::Entity::find()
        .filter(ballot_tokens::Column::ElectionId.eq(election_uuid))
        .all(db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query tokens: {}", e)))?;

    let all_ballots = ballots::Entity::find()
        .filter(ballots::Column::ElectionId.eq(election_uuid))
        .all(db)
        .await
        .map_err(|e| Error::Internal(format!("Failed to query ballots: {}", e)))?;

    let now = Utc::now();
    if now < election.end_time {
        return Err(Error::NotFound);
    }

    let ballots_for_counting: Vec<counting::Ballot> = all_ballots
        .iter()
        .filter_map(|b| b.data.clone().map(|data| data.into_ballot()))
        .collect();

    let result = if !ballots_for_counting.is_empty() {
        Some(
            crate::get_or_compute_result(
                db,
                election_uuid,
                election.candidates.0.clone(),
                election.num_seats,
                ballots_for_counting,
                &election.config,
            )
            .await?,
        )
    } else {
        None
    };

    let candidates = &election.candidates.0;

    let tokens_len = all_tokens.len();
    let ballots_len = all_ballots.len();
    let cast = all_ballots.iter().filter(|b| b.data.is_some()).count();

    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(cursor);

    write_text(&mut zip, "README.md", &generate_readme(&election, tokens_len, cast, ballots_len))?;

    write_text(&mut zip, "election.json", &serde_json::to_string_pretty(&election_config_json(&election)).unwrap())?;

    write_text(&mut zip, "tokens.json", &serde_json::to_string_pretty(&tokens_json_data(all_tokens, include_emails)).unwrap())?;

    write_text(&mut zip, "ballots.json", &serde_json::to_string_pretty(&ballots_json_data(all_ballots)).unwrap())?;

    if let Some(ref result) = result {
        write_text(&mut zip, "results.json", &serde_json::to_string_pretty(result).unwrap())?;

        write_text(&mut zip, "report.html", &generate_report_html(&election, result, candidates))?;
    }

    let cursor = zip.finish().map_err(|e| Error::Internal(format!("Failed to finalize ZIP: {}", e)))?;
    Ok(cursor.into_inner())
}

fn write_text(zip: &mut ZipWriter<std::io::Cursor<Vec<u8>>>, name: &str, content: &str) -> Result<(), Error> {
    let opts: SimpleFileOptions = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    zip.start_file(name, opts)
        .map_err(|e| Error::Internal(format!("Failed to start file in ZIP: {}", e)))?;
    zip.write_all(content.as_bytes())
        .map_err(|e| Error::Internal(format!("Failed to write to ZIP: {}", e)))?;
    Ok(())
}

fn generate_readme(
    election: &elections::entity::Model,
    tokens_len: usize,
    cast: usize,
    ballots_len: usize,
) -> String {
    let abstained = ballots_len - cast;

    let desc_line = election
        .description
        .as_ref()
        .map(|d| format!("- **Description**: {d}\n"))
        .unwrap_or_default();

    format!(
        "\
# Election Export: {title}

This archive contains the complete data for this election.

## Files

### `election.json`
Election configuration and metadata in JSON format.
- `title` — Election title
- `description` — Optional description
- `candidates` — List of candidate names
- `seats` — Number of seats to fill
- `election_type` — Voting algorithm used (`stv-md`, `stv-md-coperland`, `stv-md-grouped`, or `schulze`)
- `start_time` / `end_time` — Election period (ISO 8601)
- `config` — System-specific configuration, tagged by `election_type` (e.g. group definitions for `stv-md-grouped`)

### `tokens.json`
All ballot tokens issued for this election.
- `id` — Token identifier (UUID)
- `created_at` — When the token was created
- `sent_at` — When the token email was sent (null if not sent)
- `converted_at` — When the token was redeemed (null if not redeemed)
- `email` — Recipient email address (only included when include_emails is set)
- `candidate_groups` — (Grouped elections only) Group assignment per candidate

### `ballots.json`
All ballots (cast and abstained).
- `id` — Unique ballot identifier
- `data` — The cast ballot payload, tagged by `election_type` (null = abstained). For STV elections the payload has an `election_type` field and a `ranks` field; `ranks` is an array of rank preferences per candidate position (null = unranked). The index in the array corresponds to the candidate position in `election.json` candidates list. The value is the rank number (0-based), or null if the voter did not rank that candidate.\n

### `results.json`
Full election results including elected candidates, pairwise comparison matrix (Copeland), and the complete structured counting log.
- `type` — Result type (`stv-md`, `stv-md-coperland`, `stv-md-grouped`, or `schulze`)
- `elected` — List of elected candidates in order (a single winner for `schulze`)
- `order` — (Copeland only) Final candidate ranking by Copeland score
- `pairwise_matrix` — (Copeland and Schulze) Head-to-head comparison matrix
- `beatpaths` — (Schulze only) Strongest-path strengths `p[i][j]`; i beats j iff `p[i][j] > p[j][i]`
- `log` — (Non-grouped only) Structured counting log with per-round actions, candidate counts, and stats
- `groups` — (Grouped only) Group configuration
- `group_results` — (Grouped only) Per-group results with sub-election, log, and elected candidates

### `report.html`
Human-readable election report. Self-contained HTML (no external dependencies). Open in any browser and print to PDF if desired.

## Summary

- **Election UUID**: {uuid}
- **Title**: {title}
{desc_line}\
- **Candidates**: {candidates}
- **Seats**: {seats}
- **Election type**: {etype}
- **Start**: {start}
- **End**: {end}
- **Tokens issued**: {tokens}
- **Ballots cast**: {cast}
- **Abstained**: {abstained}
",
        title = election.title,
        uuid = election.uuid,
        desc_line = desc_line,
        candidates = election.candidates.0.join(", "),
        seats = election.num_seats,
        etype = election.config.election_type().as_str(),
        start = election.start_time.to_rfc3339(),
        end = election.end_time.to_rfc3339(),
        tokens = tokens_len,
        cast = cast,
        abstained = abstained,
    )
}

#[derive(serde::Serialize)]
struct ExportElectionJson {
    title: String,
    description: Option<String>,
    candidates: Vec<String>,
    seats: u32,
    election_type: String,
    start_time: DateTime<Utc>,
    end_time: DateTime<Utc>,
    config: crate::counting::SystemConfig,
}

fn election_config_json(election: &elections::entity::Model) -> ExportElectionJson {
    ExportElectionJson {
        title: election.title.clone(),
        description: election.description.clone(),
        candidates: election.candidates.0.clone(),
        seats: election.num_seats,
        election_type: election.config.election_type().as_str().to_string(),
        start_time: election.start_time,
        end_time: election.end_time,
        config: election.config.clone(),
    }
}

#[derive(serde::Serialize)]
struct BallotJsonEntry {
    id: String,
    data: Option<crate::counting::CastBallotRequest>,
}

#[derive(serde::Serialize)]
struct TokenJsonEntry {
    id: String,
    created_at: DateTime<Utc>,
    sent_at: Option<DateTime<Utc>>,
    converted_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
}

fn tokens_json_data(tokens: Vec<ballot_tokens::Model>, include_emails: bool) -> Vec<TokenJsonEntry> {
    tokens
        .into_iter()
        .map(|t| TokenJsonEntry {
            id: t.id,
            created_at: t.created_at,
            sent_at: t.sent_at,
            converted_at: t.converted_at,
            email: if include_emails { t.email } else { None },
        })
        .collect()
}

fn ballots_json_data(ballots: Vec<ballots::Model>) -> Vec<BallotJsonEntry> {
    ballots
        .into_iter()
        .map(|b| BallotJsonEntry {
            id: b.id,
            data: b.data,
        })
        .collect()
}

fn action_type_label(at: &CountingLogActionType) -> String {
    match at {
        CountingLogActionType::BeginCount => "Begin Count".to_string(),
        CountingLogActionType::Elect { candidate, reason } => {
            format!("Elect ({}): {}", reason, candidate)
        }
        CountingLogActionType::Iterate { reason } => format!("Iterate ({})", reason),
        CountingLogActionType::Defeat { candidate, reason } => {
            format!("Defeat ({}): {}", reason, candidate)
        }
        CountingLogActionType::BreakTie { candidates, defeated } => {
            let cs = candidates.join(", ");
            format!("Break tie (defeat): [{}] -> {}", cs, defeated)
        }
        CountingLogActionType::Lot { candidates, winner } => {
            let cs = candidates.join(", ");
            format!("Lot (seeded tie-break): [{}] -> {}", cs, winner)
        }
        CountingLogActionType::CountComplete => "Count Complete".to_string(),
    }
}

fn status_label(s: &CountingLogCandidateStatus) -> &'static str {
    match s {
        CountingLogCandidateStatus::Elected => "Elected",
        CountingLogCandidateStatus::Hopeful => "Hopeful",
        CountingLogCandidateStatus::Defeated => "Defeated",
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn generate_report_html(
    election: &elections::entity::Model,
    result: &counting::ElectionResult,
    candidates: &[String],
) -> String {
    let title = html_escape(&election.title);
    let desc = election
        .description
        .as_ref()
        .map(|d| format!("<p class=\"desc\">{}</p>", html_escape(d)))
        .unwrap_or_default();

    let candidate_groups: &[String] = match &election.config {
        counting::SystemConfig::StvMdGrouped(cfg) => cfg.candidate_groups(),
        _ => &[],
    };
    let candidates_html: String = candidates
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let group_info = candidate_groups.get(i)
                .map(|g| format!(" &mdash; <em>{}</em>", html_escape(g)))
                .unwrap_or_default();
            format!("<li><strong>{}</strong> (index {}){}</li>", html_escape(c), i, group_info)
        })
        .collect::<Vec<_>>()
        .join("\n");

    let mut results_html = String::new();

    if let counting::ElectionResult::StvMdGrouped(r) = result {
        // Grouped results: show per-group sections
        let mut groups_info = String::new();
        if !r.groups.is_empty() {
            groups_info.push_str("<h2>Groups</h2>\n<ul>\n");
            for g in &r.groups {
                groups_info.push_str(&format!(
                    "<li><strong>{}</strong> &mdash; {} seats</li>\n",
                    html_escape(g.name()), g.seats()
                ));
            }
            groups_info.push_str("</ul>\n");
        }
        results_html.push_str(&groups_info);

        for gr in &r.group_results {
            results_html.push_str(&format!(
                "<h2>Group: {} ({} seats)</h2>\n",
                html_escape(&gr.group), gr.seats
            ));

            // Elected from this group
            let mut gr_elected_rows = String::new();
            for (i, e) in gr.elected.iter().enumerate() {
                gr_elected_rows.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td></tr>\n",
                    i + 1,
                    html_escape(&e.candidate)
                ));
            }
            results_html.push_str(&format!(
                "<h3>Elected</h3>\n<table>\n<tr><th>Seat</th><th>Candidate</th></tr>\n{}</table>\n",
                gr_elected_rows
            ));

            // Sub-election candidates for this group
            let gr_candidates: String = gr.election.candidates().iter()
                .enumerate()
                .map(|(i, c)| format!("<li><strong>{}</strong> (index {})</li>", html_escape(c), i))
                .collect::<Vec<_>>()
                .join("\n");
            results_html.push_str("<h3>Group Candidates</h3>\n<ul>");
            results_html.push_str(&gr_candidates);
            results_html.push_str("</ul>\n");

            // Counting log for this group
            results_html.push_str(&format!("<h3>Counting Log</h3>\n{}", generate_log_html(&gr.log, gr.election.candidates())));
        }
    } else {
        // Non-grouped results
        let mut elected_rows = String::new();
        for (i, e) in result.elected().iter().enumerate() {
            elected_rows.push_str(&format!(
                "<tr><td>{}</td><td>{}</td></tr>\n",
                i + 1,
                html_escape(&e.candidate)
            ));
        }

        let elected_table = if result.elected().is_empty() {
            "<p>No candidates were elected.</p>\n".to_string()
        } else {
            format!(
                "<table>\n<tr><th>Seat</th><th>Candidate</th></tr>\n{}</table>\n",
                elected_rows
            )
        };

        results_html.push_str(&elected_table);

        let mut pairwise_html = String::new();
        let pairwise = match result {
            counting::ElectionResult::StvMdCoperland(r) => Some((&r.pairwise_matrix, "Copeland")),
            counting::ElectionResult::Schulze(r) => Some((&r.pairwise_matrix, "Schulze")),
            _ => None,
        };
        if let Some((matrix, label)) = pairwise {
            pairwise_html.push_str(&format!(
                "<h2>Pairwise Comparison Matrix ({label})</h2>\n"
            ));
            pairwise_html.push_str("<table>\n<tr><th></th>");
            for c in candidates {
                pairwise_html.push_str(&format!("<th>{}</th>", html_escape(c)));
            }
            pairwise_html.push_str("</tr>\n");
            for (i, row) in matrix.iter().enumerate() {
                pairwise_html.push_str(&format!("<tr><th>{}</th>", html_escape(&candidates[i])));
                for (j, val) in row.iter().enumerate() {
                    let cls = if i == j {
                        " class=\"diag\""
                    } else if *val > matrix[j][i] {
                        " class=\"win\""
                    } else if *val < matrix[j][i] {
                        " class=\"loss\""
                    } else {
                        ""
                    };
                    pairwise_html.push_str(&format!("<td{}>{}</td>", cls, val));
                }
                pairwise_html.push_str("</tr>\n");
            }
            pairwise_html.push_str("</table>\n");
        }
        if let counting::ElectionResult::Schulze(r) = result {
            let matrix = &r.beatpaths;
            pairwise_html.push_str("<h2>Beatpath Strengths (strongest path from row to column)</h2>\n");
            pairwise_html.push_str("<table>\n<tr><th></th>");
            for c in candidates {
                pairwise_html.push_str(&format!("<th>{}</th>", html_escape(c)));
            }
            pairwise_html.push_str("</tr>\n");
            for (i, row) in matrix.iter().enumerate() {
                pairwise_html.push_str(&format!("<tr><th>{}</th>", html_escape(&candidates[i])));
                for (j, val) in row.iter().enumerate() {
                    let cls = if i == j {
                        " class=\"diag\""
                    } else if *val > matrix[j][i] {
                        " class=\"win\""
                    } else if *val < matrix[j][i] {
                        " class=\"loss\""
                    } else {
                        ""
                    };
                    pairwise_html.push_str(&format!("<td{}>{}</td>", cls, val));
                }
                pairwise_html.push_str("</tr>\n");
            }
            pairwise_html.push_str("</table>\n");
        }
        results_html.push_str(&pairwise_html);

        let log = match result {
            counting::ElectionResult::StvMd(r) => Some(&r.log),
            counting::ElectionResult::StvMdCoperland(r) => Some(&r.log),
            counting::ElectionResult::StvMdGrouped(_) => None,
            counting::ElectionResult::Schulze(r) => Some(&r.log),
        };
        if let Some(log) = log {
            results_html.push_str(&format!(
                "<h2>Counting Log</h2>\n{}",
                generate_log_html(log, candidates)
            ));
        }
    }

    format!(
        "<!DOCTYPE html>
<html lang=\"en\">
<head>
<meta charset=\"UTF-8\">
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">
<title>Election Report &mdash; {title}</title>
<style>
  body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif; margin: 2em; color: #1a1a1a; background: #fff; line-height: 1.5; }}
  h1 {{ border-bottom: 2px solid #333; padding-bottom: 0.3em; }}
  h2 {{ margin-top: 1.5em; border-bottom: 1px solid #ccc; padding-bottom: 0.2em; }}
  h3 {{ margin-top: 1.2em; color: #444; }}
  .desc {{ font-size: 1.1em; color: #555; }}
  table {{ border-collapse: collapse; margin: 1em 0; min-width: 300px; }}
  th, td {{ border: 1px solid #ccc; padding: 0.5em 0.8em; text-align: left; }}
  th {{ background: #f5f5f5; font-weight: 600; }}
  .diag {{ background: #eee; text-align: center; }}
  .win {{ background: #d4edda; text-align: center; }}
  .loss {{ background: #f8d7da; text-align: center; }}
  .round {{ background: #e8f0fe; font-weight: 600; }}
  .action {{ background: #fef9e7; }}
  .action-type {{ font-style: italic; }}
  .candidate-row td {{ padding: 0.3em 0.8em; }}
  .candidate-row td:first-child {{ padding-left: 2em; }}
  .stats {{ font-size: 0.9em; color: #666; }}
  .elected {{ color: #155724; font-weight: 600; }}
  .defeated {{ color: #721c24; }}
  .hopeful {{ color: #333; }}
  ul {{ list-style: none; padding: 0; }}
  li {{ margin: 0.3em 0; }}
  @media print {{ body {{ margin: 1em; }} }}
</style>
</head>
<body>
<h1>{title}</h1>
{desc}
<div class=\"meta\">
<p><strong>Election UUID:</strong> {uuid}</p>
<p><strong>Election type:</strong> {etype}</p>
<p><strong>Seats:</strong> {seats}</p>
<p><strong>Start:</strong> {start}</p>
<p><strong>End:</strong> {end}</p>
</div>

<h2>Candidates</h2>
<ul>{candidates_html}</ul>

<h2>Results</h2>
{results_html}
</body>
</html>",
        title = title,
        desc = desc,
        uuid = html_escape(&election.uuid),
        etype = html_escape(election.config.election_type().as_str()),
        seats = election.num_seats,
        start = election.start_time.to_rfc3339(),
        end = election.end_time.to_rfc3339(),
        candidates_html = candidates_html,
        results_html = results_html,
    )
}

fn generate_log_html(log: &log::CountingLog, candidates: &[String]) -> String {
    let mut html = String::new();

    for round in &log.rounds {
        html.push_str(&format!("<h3>Round {}</h3>\n", round.round_number));

        for action in &round.actions {
            let action_str = html_escape(&action_type_label(&action.action_type));
            html.push_str(&format!(
                "<table>\n<tr class=\"action\"><th colspan=\"4\">Action: {}</th><th colspan=\"3\" class=\"stats\">Quota: {} | Total: {} | Surplus: {}</th></tr>\n",
                action_str,
                html_escape(&action.stats.quota),
                html_escape(&action.stats.total),
                html_escape(&action.stats.surplus),
            ));
            html.push_str("<tr><th>Candidate</th><th>Status</th><th>Votes</th></tr>\n");

            // candidate_counts is index-aligned with `candidates`.
            for (name, cc) in candidates.iter().zip(&action.candidate_counts) {
                let status_cls = match cc.status {
                    CountingLogCandidateStatus::Elected => "elected",
                    CountingLogCandidateStatus::Defeated => "defeated",
                    CountingLogCandidateStatus::Hopeful => "hopeful",
                };
                html.push_str(&format!(
                    "<tr class=\"candidate-row\"><td>{}</td><td class=\"{}\">{}</td><td>{}</td></tr>\n",
                    html_escape(name),
                    status_cls,
                    status_label(&cc.status),
                    html_escape(&cc.votes),
                ));
            }

            html.push_str("</table>\n");
        }
    }

    html
}
