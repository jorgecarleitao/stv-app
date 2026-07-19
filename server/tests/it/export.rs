use std::io::Read;

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

fn read_zip_entry(archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str) -> String {
    let mut file = archive.by_name(name).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    content
}

#[tokio::test]
async fn test_export_open_election_returns_404() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Open Election",
            "candidates": ["A", "B", "C"],
            "num_seats": 1,
            "election_type": "stv-md",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let eid = create_resp.json::<serde_json::Value>()["uuid"].as_str().unwrap().to_string();

    let resp = server.get(&format!("/api/elections/{}/export", eid)).await;
    assert_eq!(resp.status_code(), 404);

    Ok(())
}

#[tokio::test]
async fn test_export_closed_election() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Closed Election",
            "candidates": ["Apple", "Banana"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let election = create_resp.json::<serde_json::Value>();
    let eid = election["uuid"].as_str().unwrap().to_string();
    let aid = election["admin_uuid"].as_str().unwrap().to_string();

    let tokens_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", eid, aid))
        .json(&json!(19))
        .await;
    assert_eq!(tokens_resp.status_code(), 200);
    let tokens = tokens_resp.json::<Vec<String>>();
    assert_eq!(tokens.len(), 19);

    for (i, tid) in tokens.iter().enumerate() {
        let redeem = server
            .post(&format!("/api/elections/{}/tokens/{}/redeem", eid, tid))
            .await;
        assert_eq!(redeem.status_code(), 200);
        let bid = redeem.json::<String>();

        let ranks = if i < 10 { json!([0, 1]) } else { json!([1, 0]) };
        let vote = server
            .put(&format!("/api/elections/{}/ballot/{}", eid, bid))
            .json(&json!({ "ranks": ranks }))
            .await;
        assert_eq!(vote.status_code(), 200);
    }

    let update_resp = server
        .put(&format!("/api/elections/{}/admin/{}", eid, aid))
        .json(&json!({
            "title": "Closed Election",
            "candidates": ["Apple", "Banana"],
            "num_seats": 1,
            "election_type": "stv-md-coperland",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2000-01-01T00:00:00Z"
        }))
        .await;
    assert_eq!(update_resp.status_code(), 200);

    let resp = server.get(&format!("/api/elections/{}/export", eid)).await;
    assert_eq!(resp.status_code(), 200, "export status");
    assert!(resp.content_type().contains("zip"));

    let cursor = std::io::Cursor::new(resp.as_bytes().as_ref());
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let names: Vec<String> = archive.file_names().map(|n| n.to_string()).collect();
    for required in &["README.md", "election.json", "ballots.json", "results.json", "report.html"] {
        assert!(names.contains(&required.to_string()), "missing {}", required);
    }

    let readme = read_zip_entry(&mut archive, "README.md");
    assert!(readme.contains("Election Export"));
    assert!(readme.contains("Apple"));
    assert!(readme.contains("Banana"));
    assert!(readme.contains("Ballots cast"));

    let election_json = read_zip_entry(&mut archive, "election.json");
    let election_data: serde_json::Value =
        serde_json::from_str(&election_json).map_err(|e| e.to_string())?;
    assert_eq!(election_data["title"], "Closed Election");
    assert_eq!(election_data["candidates"].as_array().unwrap().len(), 2);
    assert_eq!(election_data["seats"], 1);
    assert_eq!(election_data["number_of_ballots"], 19);
    assert!(election_data["ballot_ids"].is_array());
    assert_eq!(election_data["ballot_ids"].as_array().unwrap().len(), 19);

    let ballots_json = read_zip_entry(&mut archive, "ballots.json");
    let ballots_data: Vec<serde_json::Value> =
        serde_json::from_str(&ballots_json).map_err(|e| e.to_string())?;
    assert_eq!(ballots_data.len(), 19);
    for b in &ballots_data {
        assert!(b["id"].is_string());
        assert!(b["ranks"].is_array());
    }

    let results_json = read_zip_entry(&mut archive, "results.json");
    let results_data: serde_json::Value =
        serde_json::from_str(&results_json).map_err(|e| e.to_string())?;
    assert_eq!(results_data["type"], "stv-md-coperland");
    assert_eq!(results_data["elected"].as_array().unwrap().len(), 1);
    assert_eq!(results_data["elected"][0]["candidate"], "Apple");
    assert!(results_data.get("log").is_some());
    assert!(results_data.get("order").is_some());
    assert!(results_data.get("pairwise_matrix").is_some());

    let report_html = read_zip_entry(&mut archive, "report.html");
    assert!(report_html.contains("Closed Election"));
    assert!(report_html.contains("Apple"));
    assert!(report_html.contains("<style>"));
    assert!(!report_html.contains("<script"));
    assert!(!report_html.contains("http://"));
    assert!(!report_html.contains("https://"));

    Ok(())
}

#[tokio::test]
async fn test_export_no_ballots() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Empty Election",
            "candidates": ["X", "Y"],
            "num_seats": 1,
            "election_type": "stv-md",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2000-01-01T00:00:00Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let eid = create_resp.json::<serde_json::Value>()["uuid"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = server.get(&format!("/api/elections/{}/export", eid)).await;
    assert_eq!(resp.status_code(), 200);

    let cursor = std::io::Cursor::new(resp.as_bytes().as_ref());
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let ballots_json = read_zip_entry(&mut archive, "ballots.json");
    assert_eq!(ballots_json, "[]");

    let election_json = read_zip_entry(&mut archive, "election.json");
    let election_data: serde_json::Value =
        serde_json::from_str(&election_json).map_err(|e| e.to_string())?;
    assert_eq!(election_data["number_of_ballots"], 0);
    assert!(election_data["ballot_ids"].is_array());
    assert!(election_data["ballot_ids"].as_array().unwrap().is_empty());

    Ok(())
}

#[tokio::test]
async fn test_export_abstentions() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    let create_resp = server
        .post("/api/elections")
        .json(&json!({
            "title": "Abstention Test",
            "candidates": ["A", "B"],
            "num_seats": 1,
            "election_type": "stv-md",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2099-12-31T23:59:59Z"
        }))
        .await;
    assert_eq!(create_resp.status_code(), 200);
    let election = create_resp.json::<serde_json::Value>();
    let eid = election["uuid"].as_str().unwrap().to_string();
    let aid = election["admin_uuid"].as_str().unwrap().to_string();

    let tokens_resp = server
        .post(&format!("/api/elections/{}/admin/{}/tokens", eid, aid))
        .json(&json!(2))
        .await;
    assert_eq!(tokens_resp.status_code(), 200);
    let tokens = tokens_resp.json::<Vec<String>>();

    let redeem = server
        .post(&format!("/api/elections/{}/tokens/{}/redeem", eid, tokens[0]))
        .await;
    assert_eq!(redeem.status_code(), 200);
    let bid = redeem.json::<String>();
    let vote = server
        .put(&format!("/api/elections/{}/ballot/{}", eid, bid))
        .json(&json!({ "ranks": [0, 1] }))
        .await;
    assert_eq!(vote.status_code(), 200);

    let redeem2 = server
        .post(&format!("/api/elections/{}/tokens/{}/redeem", eid, tokens[1]))
        .await;
    assert_eq!(redeem2.status_code(), 200);

    let update_resp = server
        .put(&format!("/api/elections/{}/admin/{}", eid, aid))
        .json(&json!({
            "title": "Abstention Test",
            "candidates": ["A", "B"],
            "num_seats": 1,
            "election_type": "stv-md",
            "start_time": "2024-01-01T00:00:00Z",
            "end_time": "2000-01-01T00:00:00Z"
        }))
        .await;
    assert_eq!(update_resp.status_code(), 200);

    let resp = server.get(&format!("/api/elections/{}/export", eid)).await;
    assert_eq!(resp.status_code(), 200);

    let cursor = std::io::Cursor::new(resp.as_bytes().as_ref());
    let mut archive = zip::ZipArchive::new(cursor).map_err(|e| e.to_string())?;

    let readme = read_zip_entry(&mut archive, "README.md");
    assert!(readme.contains("Ballots cast"), "readme: {}", readme);
    assert!(readme.contains("Abstained"), "readme: {}", readme);

    let ballots_json = read_zip_entry(&mut archive, "ballots.json");
    let ballots_data: Vec<serde_json::Value> =
        serde_json::from_str(&ballots_json).map_err(|e| e.to_string())?;

    assert_eq!(ballots_data.len(), 2);

    assert!(ballots_data[0]["ranks"].is_array());
    assert_eq!(ballots_data[0]["ranks"][0], 0);
    assert_eq!(ballots_data[0]["ranks"][1], 1);

    assert!(ballots_data[1]["ranks"].is_null());

    Ok(())
}

#[tokio::test]
async fn test_export_not_found() -> Result<(), String> {
    let db = setup_db().await?;
    let app = create_app(db).map_err(|e| e.to_string())?;
    let server = TestServer::builder().mock_transport().build(app).unwrap();

    let resp = server
        .get("/api/elections/nonexistent-uuid/export")
        .await;
    assert_eq!(resp.status_code(), 404);

    Ok(())
}
