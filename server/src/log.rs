use num::Zero;
use serde::{Deserialize, Serialize};
use stv_rs::arithmetic::BigFixedDecimal9;
use stv_rs::types::{ElectedReason, NotElectedReason, TranscriptEvent};
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
    /// A candidate has been elected, with the reason
    /// (`exceeds_quota` or `remaining`)
    Elect { candidate: String, reason: String },
    /// Surplus votes have been redistributed; the reason says how the
    /// iteration ended (`elected`, `stable_surplus`, `low_surplus`)
    Iterate { reason: String },
    /// A candidate has been defeated, with the reason
    /// (`stable_surplus`, `low_surplus` or `remaining`)
    Defeat { candidate: String, reason: String },
    /// A tie between candidates was broken by the tie-break order.
    /// stv-rs defaults the tie order to candidate list order (the app does
    /// not override it), so the earliest-listed tied candidate is defeated.
    BreakTie { candidates: Vec<String>, defeated: String },
    /// A tie between candidates was broken by a seeded random lot
    /// (Schulze), selecting the winner. The seed is the election's
    /// `tie_break_seed`, stored in the election config, so the draw
    /// is reproducible and auditable.
    Lot { candidates: Vec<String>, winner: String },
    /// The count is complete
    CountComplete,
}

/// Vote count for a single candidate at a point in the count.
/// Index-aligned with [`CountingLog::candidates`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct CountingLogCandidateCount {
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

fn elected_reason(reason: &ElectedReason) -> String {
    match reason {
        ElectedReason::ExceedsQuota => "exceeds_quota".to_string(),
        ElectedReason::Remaining => "remaining".to_string(),
        ElectedReason::Other => "other".to_string(),
    }
}

fn not_elected_reason(reason: &NotElectedReason) -> String {
    match reason {
        NotElectedReason::StableSurplus => "stable_surplus".to_string(),
        NotElectedReason::LowSurplus => "low_surplus".to_string(),
        NotElectedReason::Remaining => "remaining".to_string(),
        NotElectedReason::Other => "other".to_string(),
    }
}

/// Builds a [`CountingLog`] from the structured transcript emitted by stv-rs
pub fn transcript_to_log(
    transcript: &[TranscriptEvent<BigFixedDecimal9>],
    candidate_names: &[String],
    mut header: CountingLogHeader,
) -> CountingLog
{
    let num_candidates = candidate_names.len();
    let mut builder = LogBuilder {
        candidate_names,
        candidates: candidate_names
            .iter()
            .map(|name| CountingLogCandidate {
                name: name.clone(),
                withdrawn: false,
            })
            .collect(),
        statuses: vec![CountingLogCandidateStatus::Hopeful; num_candidates],
        tallies: vec![BigFixedDecimal9::zero(); num_candidates],
        exhausted: BigFixedDecimal9::zero(),
        quota: BigFixedDecimal9::zero(),
        surplus: BigFixedDecimal9::zero(),
        rounds: Vec::new(),
        seen_count: false,
    };

    for (idx, event) in transcript.iter().enumerate() {
        match event {
            TranscriptEvent::Withdrawn(i) => {
                builder.candidates[*i].withdrawn = true;
                builder.statuses[*i] = CountingLogCandidateStatus::Defeated;
            }
            TranscriptEvent::KeepFactors(_) => {}
            TranscriptEvent::Count { tallies, exhausted } => {
                builder.tallies.clone_from(tallies);
                builder.exhausted = exhausted.clone();
                if builder.seen_count {
                    // The reason an iteration stopped is only known from the
                    // next election/defeat event, so look ahead.
                    let reason = transcript[idx..]
                        .iter()
                        .skip(1)
                        .find_map(|e| match e {
                            TranscriptEvent::Elected(..) => Some("elected".to_string()),
                            TranscriptEvent::NotElected(_, r) => Some(not_elected_reason(r)),
                            _ => None,
                        })
                        .unwrap_or_else(|| "count".to_string());
                    builder.push_action(CountingLogActionType::Iterate { reason });
                } else {
                    builder.seen_count = true;
                    builder.start_round(0);
                    builder.push_action(CountingLogActionType::BeginCount);
                }
            }
            TranscriptEvent::Quota(q) => builder.quota = q.clone(),
            TranscriptEvent::Surplus(s) => builder.surplus = s.clone(),
            TranscriptEvent::StartRound(round) => {
                // Transcript rounds are 1-based; CountingLog rounds are 0-based.
                builder.start_round(round.saturating_sub(1));
            }
            TranscriptEvent::Elected(i, reason) => {
                builder.statuses[*i] = CountingLogCandidateStatus::Elected;
                let action = CountingLogActionType::Elect {
                    candidate: builder.name(*i),
                    reason: elected_reason(reason),
                };
                builder.push_action(action);
            }
            TranscriptEvent::NotElected(i, reason) => {
                builder.statuses[*i] = CountingLogCandidateStatus::Defeated;
                let action = CountingLogActionType::Defeat {
                    candidate: builder.name(*i),
                    reason: not_elected_reason(reason),
                };
                builder.push_action(action);
            }
            TranscriptEvent::BreakTie { list, chosen } => {
                let action = CountingLogActionType::BreakTie {
                    candidates: list.iter().map(|i| builder.name(*i)).collect(),
                    defeated: builder.name(*chosen),
                };
                builder.push_action(action);
            }
        }
    }

    builder.start_round(0); // ensure at least one round exists
    builder.push_action(CountingLogActionType::CountComplete);
    if header.quota.is_empty() {
        header.quota = builder.quota.to_string();
    }

    CountingLog {
        header,
        candidates: builder.candidates,
        rounds: builder.rounds,
    }
}

struct LogBuilder<'a> {
    candidate_names: &'a [String],
    candidates: Vec<CountingLogCandidate>,
    statuses: Vec<CountingLogCandidateStatus>,
    tallies: Vec<BigFixedDecimal9>,
    exhausted: BigFixedDecimal9,
    quota: BigFixedDecimal9,
    surplus: BigFixedDecimal9,
    rounds: Vec<CountingLogRound>,
    seen_count: bool,
}

impl LogBuilder<'_> {
    fn name(&self, i: usize) -> String {
        self.candidate_names[i].clone()
    }

    fn start_round(&mut self, round_number: usize) {
        if self.rounds.last().map(|r| r.round_number) < Some(round_number) {
            self.rounds.push(CountingLogRound {
                round_number,
                actions: Vec::new(),
            });
        }
    }

    fn push_action(&mut self, action_type: CountingLogActionType) {
        let votes = self
            .tallies
            .iter()
            .fold(BigFixedDecimal9::zero(), |acc, t| acc + t.clone());
        let total = votes.clone() + self.exhausted.clone();
        let round = self.rounds.last_mut().expect("a round must exist");
        round.actions.push(CountingLogAction {
            action_type,
            candidate_counts: self
                .statuses
                .iter()
                .zip(&self.tallies)
                .map(|(status, votes)| CountingLogCandidateCount {
                    status: status.clone(),
                    votes: votes.to_string(),
                })
                .collect(),
            stats: CountingLogStats {
                quota: self.quota.to_string(),
                votes: votes.to_string(),
                residual: self.exhausted.to_string(),
                total: total.to_string(),
                surplus: self.surplus.to_string(),
            },
        });
    }
}
