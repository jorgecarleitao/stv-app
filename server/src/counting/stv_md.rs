use super::{Elected, Election, ElectionResult, stv};

pub fn count(election: Election) -> Result<ElectionResult, String> {
    let outcome = stv::run(election.candidates(), election.seats(), election.ballots());

    let elected = outcome
        .elected
        .into_iter()
        .map(|e| Elected {
            candidate: election.candidates()[e].clone(),
            id: e,
        })
        .collect();

    Ok(ElectionResult::StvMd {
        election,
        log: outcome.log,
        elected,
    })
}
