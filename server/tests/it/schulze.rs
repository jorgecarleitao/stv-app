use axum_test::TestServer;
use serde_json::json;

use migration::MigratorTrait;
use server::*;

async fn setup_db() -> Result<sea_orm::DbConn, String> {
    let db = sea_orm::Database::connect("sqlite::memory:")
        .await
        .map_err(|e| e.to_string())?;
    migration::Migrator::up(&db, None)
        .await
        .map_err(|e| format!("Failed to run migrations: {}", e))?;
    Ok(db)
}

/// 3x A>B>C, 2x B>C>A, 2x C>A>B: a pairwise cycle (A>B, B>C, C>A), but the
/// strongest path A>B>C has strength 5 while C>A has strength 4, so the
/// Schulze winner is A.
fn cycle_simulate_payload() -> serde_json::Value {
    json!({
        "election_type": "schulze",
        "candidates": ["A", "B", "C"],
        "seats": 1,
        "tie_break_seed": 42,
        "ballots": [
            { "votes": 3, "ranks": [0, 1, 2] },
            { "votes": 2, "ranks": [2, 0, 1] },
            { "votes": 2, "ranks": [1, 2, 0] }
        ]
    })
}

#[tokio::test]
async fn test_schulze_simulate_cycle_winner() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::new(app);

    let resp = server
        .post("/api/simulate")
        .json(&cycle_simulate_payload())
        .await;
    assert_eq!(resp.status_code(), 200, "simulate failed: {}", resp.text());
    let result = resp.json::<serde_json::Value>();
    assert_eq!(result["type"], "schulze");
    assert_eq!(result["elected"].as_array().unwrap().len(), 1);
    assert_eq!(result["elected"][0]["candidate"], "A");
    // Pairwise matrix is included for audit
    assert_eq!(result["pairwise_matrix"][0][1], 5); // A over B
    assert_eq!(result["pairwise_matrix"][2][0], 4); // C over A
    // Beatpath strengths: A's strongest path to C goes via B: min(5, 5) = 5 > 4
    assert_eq!(result["beatpaths"][0][2], 5);
    assert_eq!(result["beatpaths"][2][0], 4);
    // Log records the beatpath election
    let actions = &result["log"]["rounds"][0]["actions"];
    let elect = actions
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["action_type"].get("elect").is_some())
        .expect("expected an elect action");
    assert_eq!(elect["action_type"]["elect"]["candidate"], "A");
    assert_eq!(elect["action_type"]["elect"]["reason"], "beatpath");
    Ok(())
}

#[tokio::test]
async fn test_schulze_simulate_condorcet_winner() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::new(app);

    // A beats both B and C pairwise; no lot involved.
    let resp = server
        .post("/api/simulate")
        .json(&json!({
            "election_type": "schulze",
            "candidates": ["A", "B", "C"],
            "seats": 1,
            "tie_break_seed": 7,
            "ballots": [
                { "votes": 3, "ranks": [0, 1, 2] },
                { "votes": 1, "ranks": [1, 0, 2] },
                { "votes": 1, "ranks": [2, 1, 0] }
            ]
        }))
        .await;
    assert_eq!(resp.status_code(), 200);
    let result = resp.json::<serde_json::Value>();
    assert_eq!(result["elected"][0]["candidate"], "A");
    let actions = &result["log"]["rounds"][0]["actions"];
    assert!(
        actions
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["action_type"].get("lot").is_none()),
        "no lot action expected when there is a unique beatpath winner"
    );
    Ok(())
}

#[tokio::test]
async fn test_schulze_simulate_tie_uses_seeded_lot() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::new(app);

    // Perfectly symmetric tie between A and B.
    let payload = json!({
        "election_type": "schulze",
        "candidates": ["A", "B"],
        "seats": 1,
        "tie_break_seed": 42,
        "ballots": [
            { "votes": 1, "ranks": [0, 1] },
            { "votes": 1, "ranks": [1, 0] }
        ]
    });

    let resp = server.post("/api/simulate").json(&payload).await;
    assert_eq!(resp.status_code(), 200);
    let result = resp.json::<serde_json::Value>();
    assert_eq!(result["elected"].as_array().unwrap().len(), 1);
    let winner = result["elected"][0]["candidate"].as_str().unwrap().to_string();
    assert!(winner == "A" || winner == "B");

    // Log records the lot with both tied candidates
    let actions = &result["log"]["rounds"][0]["actions"];
    let lot = actions
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["action_type"].get("lot").is_some())
        .expect("expected a lot action");
    assert_eq!(lot["action_type"]["lot"]["winner"], winner);
    assert_eq!(
        lot["action_type"]["lot"]["candidates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    // Same seed => reproducible winner
    let resp2 = server.post("/api/simulate").json(&payload).await;
    let result2 = resp2.json::<serde_json::Value>();
    assert_eq!(result2["elected"][0]["candidate"], winner);
    Ok(())
}

#[tokio::test]
async fn test_schulze_requires_exactly_one_seat() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::new(app);

    // Simulate path: seats != 1 is rejected at parse time
    let mut payload = cycle_simulate_payload();
    payload["seats"] = json!(2);
    let resp = server.post("/api/simulate").json(&payload).await;
    assert_eq!(resp.status_code(), 400, "simulate with seats=2 should fail");

    // Create path: num_seats != 1 is rejected through to_election_with_config
    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Schulze multi-seat",
            "candidates": ["A", "B", "C"],
            "num_seats": 2,
            "election_type": "schulze",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(
        create_resp.status_code(),
        400,
        "create with num_seats=2 should fail: {}",
        create_resp.text()
    );
    Ok(())
}

#[tokio::test]
async fn test_schulze_full_workflow() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app);

    // Create a schulze election (seed omitted -> server generates one)
    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Schulze Test Election",
            "description": "Best option wins",
            "candidates": ["Apple", "Banana", "Cherry"],
            "num_seats": 1,
            "election_type": "schulze",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200, "create failed: {}", create_resp.text());
    let election = create_resp.json::<serde_json::Value>();
    let election_id = election["uuid"].as_str().unwrap().to_string();
    let admin_uuid = election["admin_uuid"].as_str().unwrap().to_string();
    let seed = election["tie_break_seed"].as_u64().expect("server should generate a seed");
    assert_eq!(election["election_type"], "schulze");

    // Tokens
    let tokens_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", election_id, admin_uuid))
        .json(&json!({ "count": 7 }))
        .await;
    assert_eq!(tokens_resp.status_code(), 200);
    let tokens = tokens_resp.json::<Vec<serde_json::Value>>();
    let token_ids: Vec<String> = tokens.iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();

    // 3x Apple>Banana>Cherry, 2x Banana>Cherry>Apple, 2x Cherry>Apple>Banana
    // (the cycle from the simulate test; Schulze winner: Apple)
    let votes: [(&str, [Option<usize>; 3]); 7] = [
        ("t0", [Some(0), Some(1), Some(2)]),
        ("t1", [Some(0), Some(1), Some(2)]),
        ("t2", [Some(0), Some(1), Some(2)]),
        ("t3", [Some(2), Some(0), Some(1)]),
        ("t4", [Some(2), Some(0), Some(1)]),
        ("t5", [Some(1), Some(2), Some(0)]),
        ("t6", [Some(1), Some(2), Some(0)]),
    ];
    for (i, (_, ranks)) in votes.iter().enumerate() {
        let redeem = server
            .post(&format!("/api/elections/{}/tokens/{}/redeem", election_id, token_ids[i]))
            .await;
        assert_eq!(redeem.status_code(), 200);
        let ballot_id = redeem.json::<String>();
        let vote = server
            .put(&format!("/api/elections/{}/ballot/{}", election_id, ballot_id))
            .json(&json!({ "election_type": "schulze", "ranks": ranks }))
            .await;
        assert_eq!(vote.status_code(), 200, "cast failed: {}", vote.text());
    }

    // A ballot with the wrong type tag is rejected
    let redeem = server
        .post(&format!("/api/elections/{}/tokens/{}/redeem", election_id, token_ids[0]))
        .await;
    // token already redeemed -> 400; create a fresh election-independent check instead:
    assert_eq!(redeem.status_code(), 400);

    // Close the election
    let update_resp = server
        .put(&format!("/api/elections/{}/admin/{}", election_id, admin_uuid))
        .json(&json!({
            "title": "Schulze Test Election",
            "description": "Best option wins",
            "candidates": ["Apple", "Banana", "Cherry"],
            "num_seats": 1,
            "election_type": "schulze",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2000-01-01T00:00:00Z"
        }))
        .await;
    assert_eq!(update_resp.status_code(), 200, "update failed: {}", update_resp.text());
    // The tie-break seed is preserved across updates that omit it
    let updated = update_resp.json::<serde_json::Value>();
    assert_eq!(updated["tie_break_seed"].as_u64(), Some(seed));

    // Results
    let resp = server.get(&format!("/api/elections/{}", election_id)).await;
    assert_eq!(resp.status_code(), 200);
    let state = resp.json::<ElectionState>();
    assert_eq!(state.casted, 7);
    let results = state.results.expect("results after end");
    assert_eq!(results.elected().len(), 1);
    assert_eq!(results.elected()[0].candidate, "Apple");
    match &results {
        counting::ElectionResult::Schulze(r) => {
            assert_eq!(r.pairwise_matrix.len(), 3);
            assert_eq!(r.log.header.rule, "Schulze");
            assert_eq!(r.log.header.seats, 1);
        }
        other => panic!("expected schulze result, got {:?}", other),
    }

    // Reproducibility: recompute via simulate with the same seed gives the same winner
    let sim = server
        .post("/api/simulate")
        .json(&json!({
            "election_type": "schulze",
            "candidates": ["Apple", "Banana", "Cherry"],
            "seats": 1,
            "tie_break_seed": seed,
            "ballots": [
                { "votes": 3, "ranks": [0, 1, 2] },
                { "votes": 2, "ranks": [2, 0, 1] },
                { "votes": 2, "ranks": [1, 2, 0] }
            ]
        }))
        .await;
    assert_eq!(sim.status_code(), 200);
    assert_eq!(sim.json::<serde_json::Value>()["elected"][0]["candidate"], "Apple");

    Ok(())
}

#[tokio::test]
async fn test_schulze_ballot_type_tag_must_match() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app);

    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Schulze tag test",
            "candidates": ["A", "B"],
            "num_seats": 1,
            "election_type": "schulze",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let election = create_resp.json::<serde_json::Value>();
    let election_id = election["uuid"].as_str().unwrap().to_string();
    let admin_uuid = election["admin_uuid"].as_str().unwrap().to_string();

    let tokens_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", election_id, admin_uuid))
        .json(&json!({ "count": 1 }))
        .await;
    let tokens = tokens_resp.json::<Vec<serde_json::Value>>();
    let token_id = tokens[0]["id"].as_str().unwrap().to_string();

    let redeem = server
        .post(&format!("/api/elections/{}/tokens/{}/redeem", election_id, token_id))
        .await;
    let ballot_id = redeem.json::<String>();

    // Wrong tag: stv-md ballot on a schulze election
    let vote = server
        .put(&format!("/api/elections/{}/ballot/{}", election_id, ballot_id))
        .json(&json!({ "election_type": "stv-md", "ranks": [0, 1] }))
        .await;
    assert_eq!(vote.status_code(), 400, "mismatched ballot tag should be rejected");

    // Correct tag works
    let vote = server
        .put(&format!("/api/elections/{}/ballot/{}", election_id, ballot_id))
        .json(&json!({ "election_type": "schulze", "ranks": [0, 1] }))
        .await;
    assert_eq!(vote.status_code(), 200, "schulze ballot should be accepted: {}", vote.text());
    Ok(())
}
