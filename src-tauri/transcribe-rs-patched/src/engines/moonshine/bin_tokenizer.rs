use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::model::MoonshineError;

/// Binary tokenizer matching C++ `BinTokenizer`.
///
/// Reads a `tokenizer.bin` file where each entry is length-prefixed:
/// - `0x00` → empty token
/// - `1..127` → byte_count = value, then that many bytes
/// - `128..` → two-byte length: `(second_byte * 128) + first_byte - 128`, then that many bytes
///
/// Token ID = index in the file.
pub struct BinTokenizer {
    tokens_to_bytes: Vec<Vec<u8>>,
    space_string: &'static str,
}

impl BinTokenizer {
    /// Load binary tokenizer from `tokenizer.bin` in the given directory.
    pub fn new(path: &Path) -> Result<Self, MoonshineError> {
        let tokenizer_path = path.join("tokenizer.bin");

        if !tokenizer_path.exists() {
            return Err(MoonshineError::TokenizerNotFound(
                tokenizer_path.display().to_string(),
            ));
        }

        let mut file = File::open(&tokenizer_path).map_err(MoonshineError::Io)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data).map_err(MoonshineError::Io)?;

        let mut tokens_to_bytes = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            let first_byte = data[offset];
            offset += 1;

            if first_byte == 0 {
                tokens_to_bytes.push(Vec::new());
                continue;
            }

            let byte_count = if first_byte < 128 {
                first_byte as usize
            } else {
                if offset >= data.len() {
                    break;
                }
                let second_byte = data[offset];
                offset += 1;
                (second_byte as usize * 128) + first_byte as usize - 128
            };

            if offset + byte_count > data.len() {
                break;
            }

            let bytes = data[offset..offset + byte_count].to_vec();
            offset += byte_count;
            tokens_to_bytes.push(bytes);
        }

        if tokens_to_bytes.is_empty() {
            return Err(MoonshineError::Tokenization(
                "No tokens found in tokenizer.bin".to_string(),
            ));
        }

        log::info!(
            "Loaded {} tokens from {:?}",
            tokens_to_bytes.len(),
            tokenizer_path
        );

        Ok(Self {
            tokens_to_bytes,
            space_string: "\u{2581}", // ▁
        })
    }

    /// Decode token IDs to text.
    ///
    /// Skips special tokens (tokens wrapped in `<..>`), replaces `▁` with space, trims result.
    pub fn decode(&self, tokens: &[i64]) -> Result<String, MoonshineError> {
        let mut result_bytes: Vec<u8> = Vec::new();

        for &token in tokens {
            let idx = token as usize;
            if idx >= self.tokens_to_bytes.len() {
                continue;
            }
            let bytes = &self.tokens_to_bytes[idx];
            if bytes.is_empty() {
                continue;
            }

            // Skip special tokens like <...>
            if bytes.len() > 2 && bytes[0] == b'<' && bytes[bytes.len() - 1] == b'>' {
                continue;
            }

            result_bytes.extend_from_slice(bytes);
        }

        let text = String::from_utf8_lossy(&result_bytes);
        let text = text.replace(self.space_string, " ");
        let text = text.trim().to_string();

        Ok(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bin_format_parsing() {
        // Simulate a small tokenizer.bin in memory
        // Token 0: empty (0x00)
        // Token 1: 3 bytes "cat" (0x03, b'c', b'a', b't')
        // Token 2: 1 byte " " (0x01, b' ')
        let data: Vec<u8> = vec![
            0x00, // token 0: empty
            0x03, b'c', b'a', b't', // token 1: "cat"
            0x01, b' ', // token 2: " "
        ];

        // Write to temp file
        let dir = std::env::temp_dir().join("bin_tokenizer_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tokenizer.bin");
        std::fs::write(&path, &data).unwrap();

        let tokenizer = BinTokenizer::new(&dir).unwrap();
        assert_eq!(tokenizer.tokens_to_bytes.len(), 3);
        assert_eq!(tokenizer.tokens_to_bytes[0], Vec::<u8>::new());
        assert_eq!(tokenizer.tokens_to_bytes[1], b"cat".to_vec());
        assert_eq!(tokenizer.tokens_to_bytes[2], b" ".to_vec());

        // Cleanup
        std::fs::remove_dir_all(&dir).ok();
    }
}
