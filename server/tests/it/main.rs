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
        .json(&json!({"count": 19}))
        .await;
    assert_eq!(tokens_resp.status_code(), 200, "Failed to create tokens");
    let tokens = tokens_resp.json::<Vec<serde_json::Value>>();
    assert_eq!(tokens.len(), 19, "Expected 19 tokens");
    let token_ids: Vec<String> = tokens.iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();

    // Step 3 & 4: Redeem tokens to create ballots and cast votes
    // Cast 10 Apple->Banana ballots
    let mut first_ballot_id = String::new();
    for i in 0..10 {
        let token_id = &token_ids[i];

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
        let token_id = &token_ids[i];

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
        .json(&json!({"count": 1}))
        .await;
    assert_eq!(tokens_resp.status_code(), 200);
    let tokens = tokens_resp.json::<Vec<serde_json::Value>>();
    let token_id = tokens[0]["id"].as_str().unwrap().to_string();

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

#[tokio::test]
async fn test_create_tokens_with_recipients() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    // Create election
    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Email Token Test",
            "candidates": ["Alice", "Bob"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);

    let election = create_resp.json::<serde_json::Value>();
    let election_id = election["uuid"].as_str().unwrap();
    let admin_uuid = election["admin_uuid"].as_str().unwrap();

    // Create tokens with recipients (count inferred from array length)
    let tokens_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", election_id, admin_uuid))
        .json(&json!({
            "recipients": ["alice@example.com", "bob@example.com", "carol@example.com"]
        }))
        .await;
    assert_eq!(tokens_resp.status_code(), 200);
    let tokens = tokens_resp.json::<Vec<serde_json::Value>>();
    assert_eq!(tokens.len(), 3);

    // Verify emails are stored
    let expected_emails = ["alice@example.com", "bob@example.com", "carol@example.com"];
    for (i, token) in tokens.iter().enumerate() {
        assert_eq!(token["email"].as_str(), Some(expected_emails[i]));
        assert!(token["id"].as_str().is_some());
    }

    // Fetch tokens via admin endpoint and verify emails persisted
    let fetch_resp = server
        .get(&format!("/api/elections/{}/admin/{}/tokens", election_id, admin_uuid))
        .await;
    assert_eq!(fetch_resp.status_code(), 200);
    let fetched = fetch_resp.json::<Vec<serde_json::Value>>();
    assert_eq!(fetched.len(), 3);
    let mut fetched_emails: Vec<&str> = fetched.iter().map(|t| t["email"].as_str().unwrap()).collect();
    fetched_emails.sort();
    assert_eq!(fetched_emails, vec!["alice@example.com", "bob@example.com", "carol@example.com"]);

    // Test: duplicate recipients should be rejected
    let dup_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", election_id, admin_uuid))
        .json(&json!({
            "recipients": ["alice@example.com", "alice@example.com"]
        }))
        .await;
    assert_eq!(dup_resp.status_code(), 400);

    // Test: both count and recipients provided should be rejected
    let both_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", election_id, admin_uuid))
        .json(&json!({
            "count": 2,
            "recipients": ["a@b.com"]
        }))
        .await;
    assert_eq!(both_resp.status_code(), 400);

    // Test: neither count nor recipients provided should be rejected
    let neither_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", election_id, admin_uuid))
        .json(&json!({}))
        .await;
    assert_eq!(neither_resp.status_code(), 400);

    Ok(())
}

#[tokio::test]
async fn test_email_config_crud() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    // Create an election
    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Email Config Test",
            "candidates": ["X", "Y"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let election = create_resp.json::<serde_json::Value>();
    let election_id = election["uuid"].as_str().unwrap();
    let admin_uuid = election["admin_uuid"].as_str().unwrap();

    // GET before config exists should return 404
    let get_resp = server
        .get(&format!("/api/elections/{}/admin/{}/email-config", election_id, admin_uuid))
        .await;
    assert_eq!(get_resp.status_code(), 404);

    // Create email config
    let upsert_resp = server
        .put(&format!("/api/elections/{}/admin/{}/email-config", election_id, admin_uuid))
        .json(&json!({
            "smtp_host": "smtp.example.com",
            "smtp_port": 587,
            "smtp_username": "user@example.com",
            "smtp_password": "secret123",
            "from_name": "Election Admin",
            "from_email": "elections@example.com"
        }))
        .await;
    assert_eq!(upsert_resp.status_code(), 200);
    let upsert_body = upsert_resp.json::<serde_json::Value>();
    assert_eq!(upsert_body["smtp_host"], "smtp.example.com");
    // Password should NOT be returned
    assert!(upsert_body.get("smtp_password").is_none());

    // GET should return config without password
    let get_resp = server
        .get(&format!("/api/elections/{}/admin/{}/email-config", election_id, admin_uuid))
        .await;
    assert_eq!(get_resp.status_code(), 200);
    let get_body = get_resp.json::<serde_json::Value>();
    assert_eq!(get_body["smtp_host"], "smtp.example.com");
    assert!(get_body.get("smtp_password").is_none());

    // Update the config
    let update_resp = server
        .put(&format!("/api/elections/{}/admin/{}/email-config", election_id, admin_uuid))
        .json(&json!({
            "smtp_host": "smtp.updated.com",
            "smtp_port": 465,
            "smtp_username": "new@example.com",
            "smtp_password": "newsecret",
            "from_name": "New Admin",
            "from_email": "new@example.com"
        }))
        .await;
    assert_eq!(update_resp.status_code(), 200);
    assert_eq!(update_resp.json::<serde_json::Value>()["smtp_host"], "smtp.updated.com");

    // Delete config
    let del_resp = server
        .delete(&format!("/api/elections/{}/admin/{}/email-config", election_id, admin_uuid))
        .await;
    assert_eq!(del_resp.status_code(), 200);

    // GET should return 404 after delete
    let get_resp = server
        .get(&format!("/api/elections/{}/admin/{}/email-config", election_id, admin_uuid))
        .await;
    assert_eq!(get_resp.status_code(), 404);

    Ok(())
}

#[tokio::test]
async fn test_send_emails_without_config_returns_error() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    // Create election
    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Send Email Test",
            "candidates": ["A", "B"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let election = create_resp.json::<serde_json::Value>();
    let election_id = election["uuid"].as_str().unwrap();
    let admin_uuid = election["admin_uuid"].as_str().unwrap();

    // Try to send without config — should fail
    let send_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens/send", election_id, admin_uuid))
        .json(&json!({ "base_url": "https://example.com" }))
        .await;
    assert_eq!(send_resp.status_code(), 400);
    assert!(send_resp.text().contains("Email not configured"));

    Ok(())
}

#[tokio::test]
async fn test_send_emails_with_recipients_without_email_set_returns_error() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    // Create election
    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Recipient mismatch test",
            "candidates": ["A", "B"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let election = create_resp.json::<serde_json::Value>();
    let election_id = election["uuid"].as_str().unwrap();
    let admin_uuid = election["admin_uuid"].as_str().unwrap();

    // Create tokens without email
    let tokens_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", election_id, admin_uuid))
        .json(&json!({ "count": 1 }))
        .await;
    assert_eq!(tokens_resp.status_code(), 200);
    let tokens = tokens_resp.json::<Vec<serde_json::Value>>();
    let token_id = tokens[0]["id"].as_str().unwrap();

    // Set up SMTP config
    server
        .put(&format!("/api/elections/{}/admin/{}/email-config", election_id, admin_uuid))
        .json(&json!({
            "smtp_host": "smtp.example.com",
            "smtp_port": 587,
            "smtp_username": "user",
            "smtp_password": "pass",
            "from_name": "Admin",
            "from_email": "admin@example.com"
        }))
        .await;

    // Try to send a specific token without email set — should fail
    let send_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens/{}/send", election_id, admin_uuid, token_id))
        .json(&json!({ "base_url": "https://example.com" }))
        .await;
    assert_eq!(send_resp.status_code(), 404);

    Ok(())
}

#[tokio::test]
async fn test_create_tokens_with_usize_backward_compat() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Backward compat test",
            "candidates": ["A", "B"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let election = create_resp.json::<serde_json::Value>();
    let election_id = election["uuid"].as_str().unwrap();
    let admin_uuid = election["admin_uuid"].as_str().unwrap();

    // Create tokens with object form
    let tokens_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", election_id, admin_uuid))
        .json(&json!({"count": 5}))
        .await;
    assert_eq!(tokens_resp.status_code(), 200);
    let tokens = tokens_resp.json::<Vec<serde_json::Value>>();
    assert_eq!(tokens.len(), 5);
    for token in &tokens {
        assert!(token["id"].as_str().is_some());
        assert_eq!(token["email"], serde_json::Value::Null);
    }

    Ok(())
}

#[tokio::test]
async fn test_patch_token_mark_sent() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Patch Token Test",
            "candidates": ["A", "B"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let election = create_resp.json::<serde_json::Value>();
    let eid = election["uuid"].as_str().unwrap();
    let aid = election["admin_uuid"].as_str().unwrap();

    let tokens_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", eid, aid))
        .json(&json!({"count": 1}))
        .await;
    assert_eq!(tokens_resp.status_code(), 200);
    let tid = tokens_resp.json::<Vec<serde_json::Value>>()[0]["id"].as_str().unwrap().to_string();

    let sent_time = "2026-01-15T10:30:00Z";

    // PATCH to mark as sent
    let patch_resp = server
        .patch(&format!("/api/elections/{}/admin/{}/tokens/{}", eid, aid, tid))
        .json(&json!({"sent_at": sent_time}))
        .await;
    assert_eq!(patch_resp.status_code(), 200);
    let patch_body = patch_resp.json::<serde_json::Value>();
    assert_eq!(patch_body["sent_at"].as_str(), Some(sent_time));

    // Verify sent_at is persisted
    let tokens_resp = server
        .get(&format!("/api/elections/{}/admin/{}/tokens", eid, aid))
        .await;
    let token = &tokens_resp.json::<Vec<serde_json::Value>>()[0];
    assert_eq!(token["sent_at"].as_str(), Some(sent_time));

    Ok(())
}
