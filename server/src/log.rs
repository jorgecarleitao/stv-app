use regex::Regex;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Parsed STV counting log output
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountingLog {
    /// Election metadata extracted from the log header
    pub header: CountingLogHeader,
    /// Candidates participating in the election
    pub candidates: Vec<CountingLogCandidate>,
    /// Sequential rounds of the counting process
    pub rounds: Vec<CountingLogRound>,
}

/// Election metadata extracted from the log header
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountingLogHeader {
    /// Title of the election
    pub title: String,
    /// Name of the counting software package
    pub package_name: String,
    /// Counting rule used (e.g. Droop)
    pub rule: String,
    /// Arithmetic precision description
    pub arithmetic: String,
    /// Number of seats to be filled
    pub seats: usize,
    /// Total number of ballots cast
    pub ballots: usize,
    /// Quota required for election
    pub quota: String,
    /// Omega value used in surplus transfer calculations
    pub omega: String,
}

/// A candidate in the election
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountingLogCandidate {
    /// Candidate name
    pub name: String,
    /// Whether the candidate has withdrawn from the election
    pub withdrawn: bool,
}

/// A single round of the counting process
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountingLogRound {
    /// Zero-based round number
    pub round_number: usize,
    /// Actions taken during this round
    pub actions: Vec<CountingLogAction>,
}

/// An action taken during a counting round
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountingLogAction {
    /// The type of action performed
    pub action_type: CountingLogActionType,
    /// Candidate vote counts at the time of this action
    pub candidate_counts: Vec<CountingLogCandidateCount>,
    /// Election statistics at the time of this action
    pub stats: CountingLogStats,
}

/// The type of action performed during a counting round
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CountingLogActionType {
    /// Begin the count
    BeginCount,
    /// A candidate has been elected
    Elect { candidate: String },
    /// A candidate has been elected with remaining votes to transfer
    ElectRemaining { candidate: String },
    /// Surplus votes have been redistributed
    Iterate { reason: String },
    /// A candidate has been defeated with a reason
    Defeat { reason: String, candidate: String },
    /// A candidate has been defeated as the last remaining
    DefeatRemaining { candidate: String },
    /// A tie between candidates was broken by random selection
    BreakTie { candidates: Vec<String>, defeated: String },
    /// The count is complete
    CountComplete,
}

/// Vote count for a single candidate at a point in the count
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountingLogCandidateCount {
    /// Candidate name
    pub name: String,
    /// Candidate status at this point in the count
    pub status: CountingLogCandidateStatus,
    /// Number of votes (as a decimal string)
    pub votes: String,
}

/// Status of a candidate during the count
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum CountingLogCandidateStatus {
    /// Candidate has been elected
    Elected,
    /// Candidate is still in the running
    Hopeful,
    /// Candidate has been defeated
    Defeated,
}

/// Aggregate election statistics at a point in the count
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountingLogStats {
    /// Current quota for election
    pub quota: String,
    /// Total valid votes
    pub votes: String,
    /// Residual (exhausted) votes
    pub residual: String,
    /// Total votes including residual
    pub total: String,
    /// Current surplus votes to transfer
    pub surplus: String,
}

pub fn parse_log(raw: &str) -> CountingLog {
    let lines: Vec<&str> = raw.lines().collect();

    let re_header_title = Regex::new(r"^Election:\s*(.+)$").unwrap();
    let re_candidate = Regex::new(r"^\tAdd (eligible|withdrawn):\s*(.+)$").unwrap();
    let re_round = Regex::new(r"^Round\s+(\d+):$").unwrap();
    let re_action = Regex::new(r"^Action:\s+(.+)$").unwrap();

    let mut header = None;
    let mut candidates = Vec::new();
    let mut rounds: Vec<CountingLogRound> = Vec::new();
    let mut current_round: Option<CountingLogRound> = None;
    let mut current_action: Option<CountingLogAction> = None;

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];

        if let Some(caps) = re_header_title.captures(line) {
            let title = caps[1].to_string();
            let (parsed, new_i) = parse_header(&lines, i + 1, title);
            header = Some(parsed);
            i = new_i;
            continue;
        }

        if let Some(caps) = re_candidate.captures(line) {
            candidates.push(CountingLogCandidate {
                name: caps[2].to_string(),
                withdrawn: caps.get(1).unwrap().as_str() == "withdrawn",
            });
            i += 1;
            continue;
        }

        if let Some(caps) = re_round.captures(line) {
            let round_number: usize = caps[1].parse().unwrap_or(0);
            if let Some(cur) = current_action.take() {
                if let Some(ref mut r) = current_round {
                    r.actions.push(cur);
                }
            }
            if let Some(cur) = current_round.take() {
                rounds.push(cur);
            }
            current_round = Some(CountingLogRound {
                round_number,
                actions: Vec::new(),
            });
            i += 1;
            continue;
        }

        if let Some(caps) = re_action.captures(line) {
            if let Some(cur) = current_action.take() {
                if let Some(ref mut r) = current_round {
                    r.actions.push(cur);
                }
            }

            if current_round.is_none() {
                current_round = Some(CountingLogRound {
                    round_number: 0,
                    actions: Vec::new(),
                });
            }

            let action_str = caps[1].to_string();
            let action_type = parse_action_type(&action_str);
            current_action = Some(CountingLogAction {
                action_type,
                candidate_counts: Vec::new(),
                stats: CountingLogStats {
                    quota: String::new(),
                    votes: String::new(),
                    residual: String::new(),
                    total: String::new(),
                    surplus: String::new(),
                },
            });
            i += 1;
            continue;
        }

        if let Some(ref mut action) = current_action {
            if parse_action_line(line, action) {
                i += 1;
                continue;
            }
        }

        i += 1;
    }

    if let Some(cur) = current_action.take() {
        if let Some(ref mut r) = current_round {
            r.actions.push(cur);
        }
    }
    if let Some(cur) = current_round.take() {
        rounds.push(cur);
    }

    CountingLog {
        header: header.unwrap_or(CountingLogHeader {
            title: String::new(),
            package_name: String::new(),
            rule: String::new(),
            arithmetic: String::new(),
            seats: 0,
            ballots: 0,
            quota: String::new(),
            omega: String::new(),
        }),
        candidates,
        rounds,
    }
}

fn parse_header(lines: &[&str], mut i: usize, title: String) -> (CountingLogHeader, usize) {
    let re_header_field = Regex::new(r"^\t(.+):\s*(.+)$").unwrap();
    let re_package_name = Regex::new(r"^\t(\S+)$").unwrap();

    let mut pkg = String::new();
    let mut rule = String::new();
    let mut arithmetic = String::new();
    let mut seats = 0usize;
    let mut ballots = 0usize;
    let mut quota = String::new();
    let mut omega = String::new();

    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }
        if lines[i].starts_with("\tAdd ") {
            break;
        }
        if let Some(fc) = re_header_field.captures(lines[i]) {
            let key = fc[1].trim();
            let val = fc[2].trim();
            match key {
                "Rule" => rule = val.to_string(),
                "Arithmetic" => arithmetic = val.to_string(),
                "Seats" => seats = val.parse().unwrap_or(0),
                "Ballots" => ballots = val.parse().unwrap_or(0),
                "Quota" => quota = val.to_string(),
                "Omega" => omega = val.to_string(),
                _ => {}
            }
            i += 1;
        } else if let Some(fc) = re_package_name.captures(lines[i]) {
            pkg = fc[1].to_string();
            i += 1;
        } else {
            break;
        }
    }

    (
        CountingLogHeader {
            title,
            package_name: pkg,
            rule,
            arithmetic,
            seats,
            ballots,
            quota,
            omega,
        },
        i,
    )
}

fn parse_action_line(line: &str, action: &mut CountingLogAction) -> bool {
    let re_candidate_count_list =
        Regex::new(r"^\tDefeated:\s*(.+?)\s*\(([0-9.]+)\)$").unwrap();
    let re_candidate_count =
        Regex::new(r"^\t(Elected|Hopeful|Defeated):\s*(.+?)\s*\(([^)]+)\)$").unwrap();
    let re_stats = Regex::new(r"^\t(Quota|Votes|Residual|Total|Surplus):\s*(.+)$").unwrap();

    if let Some(caps) = re_candidate_count_list.captures(line) {
        let names_str = caps.get(1).unwrap().as_str().trim();
        let votes = caps[2].to_string();
        for name in names_str.split(", ") {
            action.candidate_counts.push(CountingLogCandidateCount {
                name: name.trim().to_string(),
                status: CountingLogCandidateStatus::Defeated,
                votes: votes.clone(),
            });
        }
        return true;
    }

    if let Some(caps) = re_candidate_count.captures(line) {
        let status = match caps.get(1).unwrap().as_str() {
            "Elected" => CountingLogCandidateStatus::Elected,
            "Hopeful" => CountingLogCandidateStatus::Hopeful,
            "Defeated" => CountingLogCandidateStatus::Defeated,
            _ => unreachable!(),
        };
        action.candidate_counts.push(CountingLogCandidateCount {
            name: caps.get(2).unwrap().as_str().trim().to_string(),
            status,
            votes: caps[3].to_string(),
        });
        return true;
    }

    if let Some(caps) = re_stats.captures(line) {
        let key = caps.get(1).unwrap().as_str();
        let val = caps[2].trim().to_string();
        match key {
            "Quota" => action.stats.quota = val,
            "Votes" => action.stats.votes = val,
            "Residual" => action.stats.residual = val,
            "Total" => action.stats.total = val,
            "Surplus" => action.stats.surplus = val,
            _ => {}
        }
        return true;
    }

    false
}

fn parse_action_type(action_str: &str) -> CountingLogActionType {
    if action_str == "Begin Count" {
        return CountingLogActionType::BeginCount;
    }
    if action_str == "Count Complete" {
        return CountingLogActionType::CountComplete;
    }
    if let Some(rest) = action_str.strip_prefix("Elect remaining: ") {
        return CountingLogActionType::ElectRemaining {
            candidate: rest.to_string(),
        };
    }
    if let Some(rest) = action_str.strip_prefix("Elect: ") {
        return CountingLogActionType::Elect {
            candidate: rest.to_string(),
        };
    }
    if let Some(rest) = action_str.strip_prefix("Defeat remaining: ") {
        return CountingLogActionType::DefeatRemaining {
            candidate: rest.to_string(),
        };
    }
    if let Some(rest) = action_str.strip_prefix("Break tie (defeat): ") {
        let parts: Vec<&str> = rest.splitn(2, " -> ").collect();
        let candidates_str = parts.first().unwrap_or(&"");
        let defeated = parts.get(1).unwrap_or(&"").to_string();
        let candidates = candidates_str
            .trim_start_matches('[')
            .trim_end_matches(']')
            .split(", ")
            .map(|s| s.to_string())
            .collect();
        return CountingLogActionType::BreakTie { candidates, defeated };
    }
    if let Some(rest) = action_str.strip_prefix("Iterate (") {
        let reason = rest.trim_end_matches(')').to_string();
        return CountingLogActionType::Iterate { reason };
    }
    if let Some(rest) = action_str.strip_prefix("Defeat (") {
        let reason_and_candidate = rest.trim_end_matches(')');
        if let Some(paren_pos) = reason_and_candidate.find("): ") {
            let reason = reason_and_candidate[..paren_pos].to_string();
            let candidate = reason_and_candidate[paren_pos + 3..].to_string();
            return CountingLogActionType::Defeat { reason, candidate };
        }
    }

    CountingLogActionType::Iterate {
        reason: action_str.to_string(),
    }
}

