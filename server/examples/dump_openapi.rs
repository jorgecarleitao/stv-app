fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "openapi.json".to_string());
    let spec = serde_json::to_string_pretty(&server::openapi())?;
    std::fs::write(&path, spec)?;
    println!("Wrote OpenAPI spec to {}", path);
    Ok(())
}
