mod export;
mod grouped;
mod log;

use axum_test::TestServer;
use serde_json::json;

use migration::MigratorTrait;
use server::*;

async fn setup_db() -> Result<sea_orm::DbConn, String> {
    let db = sea_orm::Database::connect("sqlite::memory:")
        .await
        .map_err(|e| e.to_string())?;

    // Apply migrations
    migration::Migrator::up(&db, None)
        .await
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

    Ok(db)
}

#[tokio::test]
async fn test_health() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::new(app).unwrap();

    let r = server.get("/api/health").await;
    assert_eq!(r.status_code(), 200);
    Ok(())
}

#[tokio::test]
async fn test_election_workflow() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    // Step 1: Create an election
    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "STV Test Election",
            "description": "Test election for integration tests",
            "candidates": ["Apple", "Banana"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200, "Failed to create election");

    let election_response = create_resp.json::<serde_json::Value>();
    let election_id = election_response["uuid"].as_str().unwrap().to_string();
    let admin_uuid = election_response["admin_uuid"]
        .as_str()
        .unwrap()
        .to_string();

    // Step 2: Generate 19 ballot tokens (admin action)
    let tokens_resp = server
        .post(&format!(
            "/api/elections/{}/admin/{}/tokens",
            election_id, admin_uuid
        ))
        .json(&19)
        .await;
    assert_eq!(tokens_resp.status_code(), 200, "Failed to create tokens");
    let tokens = tokens_resp.json::<Vec<String>>();
    assert_eq!(tokens.len(), 19, "Expected 19 tokens");

    // Step 3 & 4: Redeem tokens to create ballots and cast votes
    // Cast 10 Apple->Banana ballots
    let mut first_ballot_id = String::new();
    for i in 0..10 {
        let token_id = &tokens[i];

        // Redeem token to create ballot
        let redeem_resp = server
            .post(&format!(
                "/api/elections/{}/tokens/{}/redeem",
                election_id, token_id
            ))
            .await;
        assert_eq!(
            redeem_resp.status_code(),
            200,
            "Failed to redeem token {}",
            i
        );
        let ballot_id = redeem_resp.json::<String>();

        if i == 0 {
            first_ballot_id = ballot_id.clone();

            // Test: Try to redeem the same token again - should fail
            let redeem_again_resp = server
                .post(&format!(
                    "/api/elections/{}/tokens/{}/redeem",
                    election_id, token_id
                ))
                .await;
            assert_eq!(
                redeem_again_resp.status_code(),
                400,
                "Should not be able to redeem the same token twice"
            );
        }

        // Cast ballot
        let vote_resp = server
            .put(&format!(
                "/api/elections/{}/ballot/{}",
                election_id, ballot_id
            ))
            .json(&json!({ "ranks": [0, 1] }))
            .await;
        assert_eq!(vote_resp.status_code(), 200, "Failed to cast ballot {}", i);
    }

    // Cast 9 Banana->Apple ballots
    for i in 10..19 {
        let token_id = &tokens[i];

        // Redeem token to create ballot
        let redeem_resp = server
            .post(&format!(
                "/api/elections/{}/tokens/{}/redeem",
                election_id, token_id
            ))
            .await;
        assert_eq!(
            redeem_resp.status_code(),
            200,
            "Failed to redeem token {}",
            i
        );
        let ballot_id = redeem_resp.json::<String>();

        // Cast ballot
        let vote_resp = server
            .put(&format!(
                "/api/elections/{}/ballot/{}",
                election_id, ballot_id
            ))
            .json(&json!({ "ranks": [1, 0] }))
            .await;
        assert_eq!(vote_resp.status_code(), 200, "Failed to cast ballot {}", i);
    }

    // Close the election by updating end_time to the past
    let update_resp = server
        .put(&format!(
            "/api/elections/{}/admin/{}",
            election_id, admin_uuid
        ))
        .json(&json!({
            "title": "STV Test Election",
            "description": "Test election for integration tests",
            "candidates": ["Apple", "Banana"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2000-01-01T00:00:00Z"
        }))
        .await;
    assert_eq!(update_resp.status_code(), 200, "Failed to update election");

    // Fetch election results and validate
    let resp = server.get(&format!("/api/elections/{}", election_id)).await;
    assert_eq!(resp.status_code(), 200);
    let state = resp.json::<ElectionState>();

    // Verify results
    assert_eq!(state.potential_voters, 19);
    assert_eq!(state.casted, 19);
    assert!(
        state.results.is_some(),
        "Results should be available after election ends"
    );

    let results = state.results.unwrap();
    assert_eq!(results.elected().len(), 1);
    assert_eq!(results.elected()[0].candidate, "Apple");
    assert_eq!(results.election().ballots().len(), 2); // Two ballot groups: 10 Apple->Banana, 9 Banana->Apple

    // Verify ballot IDs are exposed after election ends
    assert!(
        state.election.ballots().is_some(),
        "Ballot IDs should be visible after election ends"
    );
    let ballot_ids = state.election.ballots().clone().unwrap();
    assert_eq!(ballot_ids.len(), 19);

    // Test: Try to cast a vote after election has ended - should fail
    let vote_after_close = server
        .put(&format!(
            "/api/elections/{}/ballot/{}",
            election_id, first_ballot_id
        ))
        .json(&json!({ "ranks": [1, 0] }))
        .await;
    assert_eq!(
        vote_after_close.status_code(),
        400,
        "Should not be able to cast votes after election ends"
    );

    Ok(())
}

#[tokio::test]
async fn test_cannot_modify_candidates_after_token_redemption() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    // Create an election
    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Candidate Lock Test",
            "description": "Test that candidates cannot be modified after tokens are redeemed",
            "candidates": ["Alice", "Bob", "Charlie"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);

    let election_response = create_resp.json::<serde_json::Value>();
    let election_id = election_response["uuid"].as_str().unwrap().to_string();
    let admin_uuid = election_response["admin_uuid"]
        .as_str()
        .unwrap()
        .to_string();

    // Test 1: Should be able to modify candidates before any tokens are redeemed
    let update_resp = server
        .put(&format!(
            "/api/elections/{}/admin/{}",
            election_id, admin_uuid
        ))
        .json(&json!({
            "title": "Candidate Lock Test",
            "description": "Test that candidates cannot be modified after tokens are redeemed",
            "candidates": ["Alice", "Bob", "Charlie", "Diana"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(
        update_resp.status_code(),
        200,
        "Should be able to modify candidates before tokens are redeemed"
    );

    // Generate a ballot token
    let tokens_resp = server
        .post(&format!(
            "/api/elections/{}/admin/{}/tokens",
            election_id, admin_uuid
        ))
        .json(&1)
        .await;
    assert_eq!(tokens_resp.status_code(), 200);
    let tokens = tokens_resp.json::<Vec<String>>();
    let token_id = &tokens[0];

    // Redeem the token
    let redeem_resp = server
        .post(&format!(
            "/api/elections/{}/tokens/{}/redeem",
            election_id, token_id
        ))
        .await;
    assert_eq!(redeem_resp.status_code(), 200);

    // Test 2: Should NOT be able to modify candidates after a token has been redeemed
    let update_candidates = server
        .put(&format!(
            "/api/elections/{}/admin/{}",
            election_id, admin_uuid
        ))
        .json(&json!({
            "title": "Candidate Lock Test",
            "description": "Test that candidates cannot be modified after tokens are redeemed",
            "candidates": ["Alice", "Bob", "Charlie", "Diana", "Eve"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(
        update_candidates.status_code(),
        400,
        "Should NOT be able to modify candidates after tokens are redeemed"
    );
    let error_body = update_candidates.text();
    assert!(
        error_body.contains("Cannot modify"),
        "Error message should mention modification restriction"
    );

    // Test 3: Should NOT be able to modify title after tokens are redeemed
    let update_title = server
        .put(&format!(
            "/api/elections/{}/admin/{}",
            election_id, admin_uuid
        ))
        .json(&json!({
            "title": "Modified Title",
            "description": "Test that candidates cannot be modified after tokens are redeemed",
            "candidates": ["Alice", "Bob", "Charlie", "Diana"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(
        update_title.status_code(),
        400,
        "Should NOT be able to modify title after tokens are redeemed"
    );

    // Test 4: Should NOT be able to modify description after tokens are redeemed
    let update_description = server
        .put(&format!(
            "/api/elections/{}/admin/{}",
            election_id, admin_uuid
        ))
        .json(&json!({
            "title": "Candidate Lock Test",
            "description": "Modified description",
            "candidates": ["Alice", "Bob", "Charlie", "Diana"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(
        update_description.status_code(),
        400,
        "Should NOT be able to modify description after tokens are redeemed"
    );

    // Test 5: Should still be able to modify other fields (seats, dates)
    let update_other_fields = server
        .put(&format!(
            "/api/elections/{}/admin/{}",
            election_id, admin_uuid
        ))
        .json(&json!({
            "title": "Candidate Lock Test",
            "description": "Test that candidates cannot be modified after tokens are redeemed",
            "candidates": ["Alice", "Bob", "Charlie", "Diana"],
            "num_seats": 2,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(
        update_other_fields.status_code(),
        200,
        "Should be able to modify other fields (seats, dates) even after tokens are redeemed"
    );

    Ok(())
}
