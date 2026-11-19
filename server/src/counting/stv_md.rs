use super::{Elected, RankedElection, stv};

pub fn count(election: &RankedElection) -> Result<(crate::log::CountingLog, Vec<Elected>), String> {
    let outcome = stv::run(election.candidates(), election.seats(), election.ballots());

    let elected = outcome
        .elected
        .into_iter()
        .map(|e| Elected {
            candidate: election.candidates()[e].clone(),
            id: e,
        })
        .collect();

    Ok((outcome.log, elected))
}
