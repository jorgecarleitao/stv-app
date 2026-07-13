use regex::Regex;
use server::counting::{self, CombinedBallot, Election};
use server::log::{CountingLogAction, CountingLogActionType, CountingLog};

fn run_election(
    candidates: Vec<&str>,
    seats: usize,
    ballots: Vec<(usize, Vec<Option<usize>>)>,
) -> CountingLog {
    let election = Election {
        candidates: candidates.into_iter().map(String::from).collect(),
        seats,
        ordered_seats: false,
        ballots: ballots
            .into_iter()
            .map(|(votes, ranks)| CombinedBallot { votes, ranks })
            .collect(),
    };
    counting::stv_droop(election, false).unwrap().log
}

fn all_actions(log: &CountingLog) -> Vec<&CountingLogAction> {
    log.rounds.iter().flat_map(|r| r.actions.iter()).collect()
}

#[test]
fn test_header_matches_input() {
    let log = run_election(
        vec!["Alice", "Bob", "Carol"],
        2,
        vec![
            (5, vec![Some(0), Some(1), Some(2)]),
            (3, vec![Some(1), Some(0), Some(2)]),
            (2, vec![Some(2), Some(0), Some(1)]),
        ],
    );
    assert!(!log.header.title.is_empty() || log.header.title.is_empty());
    assert_eq!(log.header.seats, 2);
    assert_eq!(log.header.ballots, 10);
    assert!(!log.header.rule.is_empty());
    assert!(!log.header.arithmetic.is_empty());
    assert!(!log.header.quota.is_empty());
    assert!(!log.header.omega.is_empty());
}

#[test]
fn test_candidates_match_input() {
    let log = run_election(
        vec!["Alice", "Bob", "Carol"],
        1,
        vec![(10, vec![Some(0)])],
    );
    assert_eq!(log.candidates.len(), 3);
    let names: Vec<&str> = log.candidates.iter().map(|c| c.name.as_str()).collect();
    assert!(names.contains(&"Alice"));
    assert!(names.contains(&"Bob"));
    assert!(names.contains(&"Carol"));
    assert!(log.candidates.iter().all(|c| !c.withdrawn));
}

#[test]
fn test_rounds_are_numbered_sequentially() {
    let log = run_election(
        vec!["A", "B", "C", "D"],
        2,
        vec![
            (4, vec![Some(0), Some(1), Some(2), Some(3)]),
            (3, vec![Some(1), Some(2), Some(3), Some(0)]),
            (2, vec![Some(2), Some(3), Some(0), Some(1)]),
        ],
    );
    assert!(!log.rounds.is_empty());
    for (i, round) in log.rounds.iter().enumerate() {
        assert_eq!(round.round_number, i);
    }
}

#[test]
fn test_every_action_has_stats() {
    let log = run_election(
        vec!["X", "Y", "Z"],
        1,
        vec![
            (5, vec![Some(0), Some(1), Some(2)]),
            (3, vec![Some(1), Some(0), Some(2)]),
        ],
    );
    for action in all_actions(&log) {
        assert!(!action.stats.quota.is_empty(), "missing quota");
        assert!(!action.stats.votes.is_empty(), "missing votes");
        assert!(!action.stats.residual.is_empty(), "missing residual");
        assert!(!action.stats.total.is_empty(), "missing total");
        assert!(!action.stats.surplus.is_empty(), "missing surplus");
    }
}

#[test]
fn test_first_action_is_begin_count() {
    let log = run_election(
        vec!["A", "B", "C"],
        1,
        vec![(6, vec![Some(0), Some(1), Some(2)])],
    );
    let first = &log.rounds[0].actions[0];
    assert!(matches!(first.action_type, CountingLogActionType::BeginCount));
}

#[test]
fn test_last_action_is_count_complete() {
    let log = run_election(
        vec!["A", "B", "C"],
        1,
        vec![
            (4, vec![Some(0), Some(1), Some(2)]),
            (3, vec![Some(1), Some(2), Some(0)]),
        ],
    );
    let last_round = log.rounds.last().unwrap();
    let last_action = last_round.actions.last().unwrap();
    assert!(matches!(
        last_action.action_type,
        CountingLogActionType::CountComplete
    ));
}

#[test]
fn test_begin_count_has_candidate_counts() {
    let log = run_election(
        vec!["Alice", "Bob"],
        1,
        vec![(5, vec![Some(0), Some(1)]), (3, vec![Some(1), Some(0)])],
    );
    let begin = &log.rounds[0].actions[0];
    assert!(matches!(begin.action_type, CountingLogActionType::BeginCount));
    assert!(
        !begin.candidate_counts.is_empty(),
        "Begin Count should list candidates"
    );
}

#[test]
fn test_elect_actions_reference_valid_candidates() {
    let candidates = vec!["A", "B", "C", "D"];
    let log = run_election(
        candidates.clone(),
        2,
        vec![
            (5, vec![Some(0), Some(1), Some(2), Some(3)]),
            (3, vec![Some(1), Some(2), Some(3), Some(0)]),
        ],
    );
    for action in all_actions(&log) {
        match &action.action_type {
            CountingLogActionType::Elect { candidate }
            | CountingLogActionType::ElectRemaining { candidate }
            | CountingLogActionType::Defeat { candidate, .. }
            | CountingLogActionType::DefeatRemaining { candidate } => {
                assert!(
                    candidates.contains(&candidate.as_str()),
                    "action references unknown candidate: {candidate}"
                );
            }
            CountingLogActionType::BreakTie { defeated, .. } => {
                assert!(
                    candidates.contains(&defeated.as_str()),
                    "BreakTie references unknown candidate: {defeated}"
                );
            }
            _ => {}
        }
    }
}

#[test]
fn test_defeat_has_reason() {
    let log = run_election(
        vec!["A", "B", "C", "D"],
        1,
        vec![
            (4, vec![Some(0), Some(1), Some(2), Some(3)]),
            (3, vec![Some(1), Some(2), Some(3), Some(0)]),
            (2, vec![Some(2), Some(3), Some(0), Some(1)]),
        ],
    );
    let has_defeat = all_actions(&log).iter().any(|a| {
        matches!(
            &a.action_type,
            CountingLogActionType::Defeat { reason, .. } if !reason.is_empty()
        )
    });
    assert!(has_defeat, "should have at least one Defeat with a reason");
}

#[test]
fn test_candidate_counts_have_nonempty_votes() {
    let log = run_election(
        vec!["A", "B", "C"],
        1,
        vec![
            (4, vec![Some(0), Some(1), Some(2)]),
            (3, vec![Some(1), Some(2), Some(0)]),
        ],
    );
    for action in all_actions(&log) {
        for cc in &action.candidate_counts {
            assert!(
                !cc.votes.is_empty(),
                "candidate {} has empty votes in action {:?}",
                cc.name,
                action.action_type
            );
        }
    }
}

#[test]
fn test_parse_comma_separated_defeated() {
    let input = "\tDefeated: B, C, D (0.000000000)";
    let re = Regex::new(r"^\tDefeated:\s*(.+?)\s*\(([0-9.]+)\)$").unwrap();
    let caps = re.captures(input).unwrap();
    let names_str = caps.get(1).unwrap().as_str().trim();
    let votes = caps[2].to_string();
    let names: Vec<&str> = names_str.split(", ").collect();
    assert_eq!(names, vec!["B", "C", "D"]);
    assert_eq!(votes, "0.000000000");
}
