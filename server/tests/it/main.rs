use axum_test::TestServer;
use serde_json::json;

use server::*;

async fn state() -> AppState {
    AppState {}
}

#[tokio::test]
async fn test_health() -> Result<(), String> {
    let state = state().await;

    let app = create_app(state).unwrap();

    let server = TestServer::new(app).unwrap();

    let r = server.get("/api/health").await;
    assert_eq!(r.status_code(), 200);
    Ok(())
}

#[tokio::test]
async fn test_election() -> Result<(), String> {
    let state = state().await;

    let app = create_app(state).unwrap();

    let server = TestServer::builder()
        .mock_transport() // This provides in-memory HTTP for testing and works with all Router setups
        .build(app)
        .unwrap();

    let r = server
        .post("/api/election")
        .json(&json!({
            "candidates": ["Apple", "Banana"],
            "seats": 1,
            "ballots": [
                {
                "votes": 10,
                "order": [[0], [1]],
                },
                {
                    "votes": 9,
                    "order": [[1], [0]],
                }
            ],
        }))
        .await;
    assert_eq!(r.status_code(), 200);
    let r = r.json::<ElectionResult>();
    assert_eq!(
        r,
        ElectionResult {
            log: "\nElection: \n\n\telection\n\tRule: Meek Parametric (omega = 1/10^6)\n\tArithmetic: exact rational arithmetic\n\tSeats: 1\n\tBallots: 19\n\tQuota: 19/2\n\tOmega: 1/1000000\n\n\tAdd eligible: Apple\n\tAdd eligible: Banana\nAction: Begin Count\n\tHopeful:  Apple (10)\n\tHopeful:  Banana (9)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 0\nRound 1:\nAction: Elect: Apple\n\tElected:  Apple (10)\n\tHopeful:  Banana (9)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 0\nAction: Iterate (elected)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 1/2\nAction: Defeat remaining: Banana\n\tElected:  Apple (10)\n\tDefeated: Banana (9)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 1/2\nAction: Count Complete\n\tElected:  Apple (19)\n\tDefeated: Banana (0)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 1/2\n\n".to_string(),
            elected: vec![Elected {
                candidate: "Apple".to_string(),
                id: 0,
            }],
        }
    );
    dbg!();
    Ok(())
}
