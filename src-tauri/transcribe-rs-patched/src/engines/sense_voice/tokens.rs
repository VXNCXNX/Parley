use base64::{engine::general_purpose::STANDARD, Engine as _};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Symbol table mapping token IDs to strings.
///
/// Loaded from a `tokens.txt` file with the format:
/// ```text
/// <blank> 0
/// <unk> 1
/// a 3
/// ...
/// ```
pub struct SymbolTable {
    id_to_sym: HashMap<i64, String>,
}

impl SymbolTable {
    /// Load symbol table from a tokens.txt file.
    pub fn load(path: &Path) -> Result<Self, std::io::Error> {
        let contents = fs::read_to_string(path)?;
        let mut id_to_sym = HashMap::new();

        for line in contents.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Format: "symbol id" or just "id" (space token)
            let parts: Vec<&str> = line.rsplitn(2, |c: char| c.is_whitespace()).collect();
            if parts.len() == 2 {
                if let Ok(id) = parts[0].parse::<i64>() {
                    id_to_sym.insert(id, parts[1].to_string());
                }
            }
        }

        log::info!("Loaded {} tokens from {:?}", id_to_sym.len(), path);
        Ok(Self { id_to_sym })
    }

    /// Decode all base64-encoded token values in-place.
    /// Used for FunASR Nano models where tokens are base64-encoded.
    /// Tokens that fail to decode (e.g. `<blank>`, `<unk>`) are left as-is.
    pub fn apply_base64_decode(&mut self) {
        for sym in self.id_to_sym.values_mut() {
            if let Ok(bytes) = STANDARD.decode(sym.as_bytes()) {
                if let Ok(decoded) = String::from_utf8(bytes) {
                    *sym = decoded;
                }
            }
        }
    }

    /// Look up a symbol by token ID.
    pub fn get(&self, id: i64) -> Option<&str> {
        self.id_to_sym.get(&id).map(|s| s.as_str())
    }

    /// Look up a symbol by token ID, returning empty string if not found.
    pub fn get_or_empty(&self, id: i64) -> &str {
        self.id_to_sym.get(&id).map(|s| s.as_str()).unwrap_or("")
    }
}
