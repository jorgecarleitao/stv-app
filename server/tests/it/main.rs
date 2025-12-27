use axum_test::TestServer;
use chrono::Utc;
use sea_orm::{ConnectionTrait, Statement};
use serde_json::json;
use std::fs;

use uuid::Uuid;

use migration::MigratorTrait;
use server::counting::{Elected, ElectionResult};
use server::*;

fn write_temp_elections_dir() -> Result<String, String> {
    let dir = std::env::temp_dir().join(format!("stv-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let yaml = r#"
id: stv-test
name: STV Test Election
candidates:
    - Apple
    - Banana
seats: 1
start_time: 2024-01-01T00:00:00Z
end_time: 2099-12-31T23:59:59Z
ballots:
    - ballot-0
    - ballot-1
    - ballot-2
    - ballot-3
    - ballot-4
    - ballot-5
    - ballot-6
    - ballot-7
    - ballot-8
    - ballot-9
    - ballot-10
    - ballot-11
    - ballot-12
    - ballot-13
    - ballot-14
    - ballot-15
    - ballot-16
    - ballot-17
    - ballot-18
"#;
    fs::write(dir.join("stv-test.yaml"), yaml).map_err(|e| e.to_string())?;

    Ok(dir
        .into_os_string()
        .into_string()
        .unwrap_or_else(|_| "".to_string()))
}

async fn setup_db() -> Result<(sea_orm::DbConn, String), String> {
    let db = sea_orm::Database::connect("sqlite::memory:")
        .await
        .map_err(|e| e.to_string())?;

    // Apply migrations using the existing migration crate
    migration::Migrator::up(&db, None)
        .await
        .map_err(|e| format!("Failed to run migrations: {}", e))?;

    let elections_path = write_temp_elections_dir()?;

    Ok((db, elections_path))
}

#[tokio::test]
async fn test_health() -> Result<(), String> {
    let (db, elections_path) = setup_db().await?;

    let app = create_app(db, elections_path).map_err(|e| e.to_string())?;

    let server = TestServer::new(app).unwrap();

    let r = server.get("/api/health").await;
    assert_eq!(r.status_code(), 200);
    Ok(())
}

#[tokio::test]
async fn test_election() -> Result<(), String> {
    let (db, elections_path) = setup_db().await?;

    // Seed 19 empty ballots directly in the DB for election "stv-test"
    for i in 0..19 {
        let id = format!("ballot-{}", i);
        let sql = format!(
            "INSERT INTO ballots (id, election_id, ballot_content) VALUES ('{}', 'stv-test', NULL)",
            id
        );
        db.execute(Statement::from_string(db.get_database_backend(), sql))
            .await
            .map_err(|e| format!("Failed to insert ballot {}: {}", i, e))?;
    }

    let app = create_app(db, elections_path).map_err(|e| e.to_string())?;

    let server = TestServer::builder().mock_transport().build(app).unwrap();

    // Cast 10 Apple->Banana ballots
    for i in 0..10 {
        let id = format!("ballot-{}", i);
        let resp = server
            .put(&format!("/api/elections/stv-test/ballot/{}", id))
            .json(&json!({ "ranks": [0, 1] }))
            .await;
        assert_eq!(resp.status_code(), 200, "failed PUT for {}", id);
    }

    // Cast 9 Banana->Apple ballots
    for i in 10..19 {
        let id = format!("ballot-{}", i);
        let resp = server
            .put(&format!("/api/elections/stv-test/ballot/{}", id))
            .json(&json!({ "ranks": [1, 0] }))
            .await;
        assert_eq!(resp.status_code(), 200, "failed PUT for {}", id);
    }

    // Fetch election results and assert exact payload (log + elected)
    let resp = server.get("/api/elections/stv-test").await;
    assert_eq!(resp.status_code(), 200);
    let state = resp.json::<ElectionState>();

    let expected_log = "\nElection: \n\n\telection\n\tRule: Meek Parametric (omega = 1/10^6)\n\tArithmetic: exact rational arithmetic\n\tSeats: 1\n\tBallots: 19\n\tQuota: 19/2\n\tOmega: 1/1000000\n\n\tAdd eligible: Apple\n\tAdd eligible: Banana\nAction: Begin Count\n\tHopeful:  Apple (10)\n\tHopeful:  Banana (9)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 0\nRound 1:\nAction: Elect: Apple\n\tElected:  Apple (10)\n\tHopeful:  Banana (9)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 0\nAction: Iterate (elected)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 1/2\nAction: Defeat remaining: Banana\n\tElected:  Apple (10)\n\tDefeated: Banana (9)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 1/2\nAction: Count Complete\n\tElected:  Apple (19)\n\tDefeated: Banana (0)\n\tQuota: 19/2\n\tVotes: 19\n\tResidual: 0\n\tTotal: 19\n\tSurplus: 1/2\n\n";

    let expected_results = Some(ElectionResult {
        log: expected_log.to_string(),
        elected: vec![Elected {
            candidate: "Apple".to_string(),
            id: 0,
        }],
    });

    use chrono::TimeZone;
    let expected_state = ElectionState {
        election: server::election_yaml::ElectionConfig {
            id: "stv-test".to_string(),
            name: "STV Test Election".to_string(),
            candidates: vec!["Apple".to_string(), "Banana".to_string()],
            seats: 1,
            start_time: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            end_time: Utc.with_ymd_and_hms(2099, 12, 31, 23, 59, 59).unwrap(),
            number_of_ballots: 19,
            ballots: None,
        },
        potential_voters: 19,
        casted: 19,
        results: expected_results,
    };

    assert_eq!(state, expected_state);
    Ok(())
}
