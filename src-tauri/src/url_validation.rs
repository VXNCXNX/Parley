//! Shared URL validation for provider endpoints.

/// Validate that a URL uses http/https and does not target SSRF-sensitive endpoints.
pub fn validate_provider_url(url: &str) -> Result<(), String> {
    let parsed =
        url::Url::parse(url).map_err(|e| format!("Invalid URL '{}': {}", url, e))?;

    match parsed.scheme() {
        "http" | "https" => {}
        scheme => {
            return Err(format!(
                "URL scheme '{}' is not allowed. Only http and https are permitted.",
                scheme
            ));
        }
    }

    // Block cloud metadata endpoints (SSRF protection)
    if let Some(host) = parsed.host_str() {
        if host == "169.254.169.254" || host == "metadata.google.internal" {
            return Err("Cloud metadata endpoints are not allowed".to_string());
        }
    }

    Ok(())
}
