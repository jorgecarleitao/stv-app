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

async fn make_server() -> Result<(TestServer, sea_orm::DbConn), String> {
    let db = setup_db().await?;
    let app = create_app(db.clone()).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();
    Ok((server, db))
}

#[tokio::test]
async fn test_create_grouped_election() -> Result<(), String> {
    let (server, _db) = make_server().await?;

    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Grouped Election Test",
            "description": "Test grouped election creation",
            "candidates": ["Alice", "Bob", "Carol", "Dave"],
            "num_seats": 4,
            "election_type": "stv-md-grouped",
            "groups": [
                { "name": "Group A", "seats": 2 },
                { "name": "Group B", "seats": 2 }
            ],
            "candidate_groups": ["Group A", "Group B", "Group A", "Group B"],
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200, "Failed to create grouped election");

    let election = create_resp.json::<serde_json::Value>();
    assert_eq!(election["election_type"], "stv-md-grouped");
    assert_eq!(election["groups"].as_array().unwrap().len(), 2);
    assert_eq!(election["groups"][0]["name"], "Group A");
    assert_eq!(election["groups"][0]["seats"], 2);
    assert_eq!(election["groups"][1]["name"], "Group B");
    assert_eq!(election["groups"][1]["seats"], 2);
    assert_eq!(election["candidate_groups"].as_array().unwrap().len(), 4);
    assert_eq!(election["candidate_groups"][0], "Group A");
    assert_eq!(election["candidate_groups"][1], "Group B");

    Ok(())
}

#[tokio::test]
async fn test_grouped_election_simulate() -> Result<(), String> {
    let (server, _db) = make_server().await?;

    let sim_resp = server
        .post("/api/simulate")
        .json(&json!({
            "candidates": ["Alice", "Bob", "Carol", "Dave", "Eve"],
            "seats": 4,
            "election_type": "stv-md-grouped",
            "groups": [
                { "name": "female", "seats": 2 },
                { "name": "male", "seats": 2 }
            ],
            "candidate_groups": ["female", "male", "female", "male", "female"],
            "ballots": [
                { "votes": 10, "ranks": [0, 3, 1, 2, 4] },
                { "votes": 8, "ranks": [1, 2, 0, 4, 3] },
                { "votes": 6, "ranks": [3, 0, 2, 1, 4] },
                { "votes": 5, "ranks": [4, 1, 3, 2, 0] }
            ]
        }))
        .await;

    assert_eq!(sim_resp.status_code(), 200, "Simulation should succeed");

    let result = sim_resp.json::<serde_json::Value>();
    assert_eq!(result["type"], "stv-md-grouped");
    assert!(result["group_results"].is_array());
    assert_eq!(result["group_results"].as_array().unwrap().len(), 2);

    // Check first group (female: Alice=0, Carol=2, Eve=4)
    let group_a = &result["group_results"][0];
    assert_eq!(group_a["group"], "female");
    assert_eq!(group_a["seats"], 2);
    assert!(group_a["elected"].is_array());
    assert_eq!(group_a["elected"].as_array().unwrap().len(), 2);

    // Check second group (male: Bob=1, Dave=3)
    let group_b = &result["group_results"][1];
    assert_eq!(group_b["group"], "male");
    assert_eq!(group_b["seats"], 2);
    assert!(group_b["elected"].is_array());
    assert_eq!(group_b["elected"].as_array().unwrap().len(), 2);

    // Flat list should contain all 4 elected
    assert!(result["elected"].is_array());
    assert_eq!(result["elected"].as_array().unwrap().len(), 4);

    // Each group's sub-election should have correct candidates
    assert_eq!(group_a["election"]["candidates"].as_array().unwrap().len(), 3);
    assert_eq!(group_b["election"]["candidates"].as_array().unwrap().len(), 2);

    // Each group should have a counting log
    assert!(group_a["log"]["rounds"].as_array().unwrap().len() > 0);
    assert!(group_b["log"]["rounds"].as_array().unwrap().len() > 0);

    Ok(())
}

#[tokio::test]
async fn test_grouped_election_seat_mismatch_rejected() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    let sim_resp = server
        .post("/api/simulate")
        .json(&json!({
            "candidates": ["Alice", "Bob", "Carol", "Dave"],
            "seats": 4,
            "election_type": "stv-md-grouped",
            "groups": [
                { "name": "Group A", "seats": 3 },
                { "name": "Group B", "seats": 2 }
            ],
            "candidate_groups": ["Group A", "Group B", "Group A", "Group B"],
            "ballots": [
                { "votes": 10, "ranks": [0, 1, 2, 3] }
            ]
        }))
        .await;

    assert_eq!(
        sim_resp.status_code(),
        500,
        "Should reject group seats not matching total seats"
    );

    Ok(())
}

#[tokio::test]
async fn test_grouped_election_empty_group_rejected() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    let sim_resp = server
        .post("/api/simulate")
        .json(&json!({
            "candidates": ["Alice"],
            "seats": 1,
            "election_type": "stv-md-grouped",
            "groups": [
                { "name": "Group A", "seats": 1 },
                { "name": "Group B", "seats": 0 }
            ],
            "candidate_groups": ["Group A"],
            "ballots": [
                { "votes": 10, "ranks": [0] }
            ]
        }))
        .await;

    assert_eq!(
        sim_resp.status_code(),
        500,
        "Should reject group with no candidates"
    );

    Ok(())
}

#[tokio::test]
async fn test_grouped_election_update() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    // Create
    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Grouped Update Test",
            "candidates": ["A", "B", "C", "D"],
            "num_seats": 4,
            "election_type": "stv-md-grouped",
            "groups": [
                { "name": "X", "seats": 2 },
                { "name": "Y", "seats": 2 }
            ],
            "candidate_groups": ["X", "Y", "X", "Y"],
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);

    let election = create_resp.json::<serde_json::Value>();
    let election_id = election["uuid"].as_str().unwrap();
    let admin_uuid = election["admin_uuid"].as_str().unwrap();

    // Update groups
    let update_resp = server
        .put(&format!(
            "/api/elections/{}/admin/{}",
            election_id, admin_uuid
        ))
        .json(&json!({
            "title": "Grouped Update Test",
            "candidates": ["A", "B", "C", "D"],
            "num_seats": 4,
            "election_type": "stv-md-grouped",
            "groups": [
                { "name": "Alpha", "seats": 1 },
                { "name": "Beta", "seats": 3 }
            ],
            "candidate_groups": ["Alpha", "Beta", "Alpha", "Beta"],
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(update_resp.status_code(), 200, "Should update groups");

    let updated = update_resp.json::<serde_json::Value>();
    assert_eq!(updated["groups"][0]["name"], "Alpha");
    assert_eq!(updated["groups"][0]["seats"], 1);
    assert_eq!(updated["groups"][1]["name"], "Beta");
    assert_eq!(updated["groups"][1]["seats"], 3);

    Ok(())
}
