use super::CombinedBallot;

pub(super) struct StvOutcome {
    pub elected: Vec<usize>,
    pub log: crate::log::CountingLog,
}

pub(super) fn ranks_to_order(ranks: &[Option<usize>]) -> Vec<Vec<usize>> {
    ranks
        .iter()
        .enumerate()
        .filter_map(|(cand, &rank)| rank.map(|r| (r, cand)))
        .fold(vec![], |mut order, (rank, cand)| {
            while order.len() <= rank {
                order.push(vec![]);
            }
            order[rank].push(cand);
            order
        })
}

pub(super) fn run(candidates: &[String], seats: usize, ballots: &[CombinedBallot]) -> StvOutcome {
    use num::BigInt;
    use stv_rs::arithmetic::BigFixedDecimal9;
    use stv_rs::types::{Ballot, Candidate, Election};

    let stv_candidates = candidates
        .iter()
        .map(|c| Candidate::new(c, false))
        .collect::<Vec<_>>();

    let stv_ballots = ballots
        .iter()
        .map(|b| Ballot::new(b.votes, ranks_to_order(&b.ranks)))
        .collect::<Vec<_>>();

    let stv_election = Election::builder()
        .title("")
        .candidates(stv_candidates)
        .num_seats(seats)
        .ballots(stv_ballots)
        .build();

    // Omega = 10^-omega_exponent, the precision floor for surplus transfers.
    let omega_exponent = 6;
    let result = stv_rs::meek::stv_droop::<
        BigInt,
        BigFixedDecimal9,
    >(
        &mut std::io::sink(),
        &stv_election,
        "election",
        omega_exponent,
        stv_rs::cli::Parallel::No,
        None,
        true,
        false,
        true,
    )
    .unwrap();

    let header = crate::log::CountingLogHeader {
        title: String::new(),
        rule: "Droop".to_string(),
        arithmetic: "BigFixedDecimal9".to_string(),
        seats,
        ballots: ballots.iter().map(|b| b.votes).sum(),
        quota: String::new(),
    };
    let log = crate::log::transcript_to_log(&result.transcript, candidates, header);

    StvOutcome {
        elected: result.elected,
        log,
    }
}
