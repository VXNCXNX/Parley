use tauri::AppHandle;

#[derive(serde::Deserialize)]
struct ChirpServiceAccountForValidation {
    client_email: String,
    private_key: String,
    project_id: String,
    token_uri: String,
}

#[tauri::command]
#[specta::specta]
pub fn change_gemini_api_key_setting(app: AppHandle, api_key: String) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    let api_key = api_key.trim();
    settings.gemini_api_key = if api_key.is_empty() {
        None
    } else {
        // Encrypt the API key before storing
        Some(crate::secret_store::encrypt_api_key(api_key))
    };
    crate::settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_gemini_model_setting(app: AppHandle, model: String) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.gemini_model = model;
    crate::settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_gemini_project_id_setting(app: AppHandle, project_id: String) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.gemini_project_id = if project_id.is_empty() {
        None
    } else {
        Some(project_id)
    };
    crate::settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_gemini_location_setting(app: AppHandle, location: String) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.gemini_location = location;
    crate::settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_chirp_service_account_setting(
    app: AppHandle,
    service_account_json: String,
) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    let service_account_json = service_account_json.trim();
    if service_account_json.is_empty() {
        settings.chirp_service_account = None;
    } else {
        let service_account: ChirpServiceAccountForValidation =
            serde_json::from_str(service_account_json)
                .map_err(|e| format!("Invalid JSON: {}", e))?;

        if service_account.client_email.trim().is_empty()
            || service_account.private_key.trim().is_empty()
            || service_account.project_id.trim().is_empty()
            || service_account.token_uri.trim().is_empty()
        {
            return Err("Invalid service account JSON: missing required Chirp fields".to_string());
        }

        settings.chirp_service_account =
            Some(crate::secret_store::encrypt_api_key(service_account_json));
    }
    crate::settings::write_settings(&app, settings);
    Ok(())
}
