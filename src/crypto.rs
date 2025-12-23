//! AES-256-CBC 加密/解密模块

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use crate::error::{Mt4Error, Result};

type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;
type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

/// AES-256-CBC 加密器
#[derive(Clone)]
pub struct Mt4Crypto {
    /// 预设的认证密钥 (用于 token)
    auth_key: [u8; 32],
    /// 会话密钥 (用于其他消息)
    session_key: Option<[u8; 32]>,
}

impl Mt4Crypto {
    /// 创建新的加密器
    pub fn new() -> Result<Self> {
        let auth_key = Self::decode_auth_key()?;
        Ok(Self {
            auth_key,
            session_key: None,
        })
    }

    /// 解码预设的认证密钥
    fn decode_auth_key() -> Result<[u8; 32]> {
        let hex_str = crate::protocol::AUTH_KEY_HEX;
        let bytes = hex::decode(hex_str)
            .map_err(|e| Mt4Error::Encryption(format!("Failed to decode auth key: {}", e)))?;

        if bytes.len() != 32 {
            return Err(Mt4Error::Encryption(format!(
                "Invalid auth key length: {} (expected 32)",
                bytes.len()
            )));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        Ok(key)
    }

    /// 设置会话密钥 (从服务器返回的 key 字段)
    pub fn set_session_key(&mut self, key_hex: &str) -> Result<()> {
        let bytes = hex::decode(key_hex)
            .map_err(|e| Mt4Error::Encryption(format!("Failed to decode session key: {}", e)))?;

        if bytes.len() != 32 {
            return Err(Mt4Error::Encryption(format!(
                "Invalid session key length: {} (expected 32)",
                bytes.len()
            )));
        }

        let mut key = [0u8; 32];
        key.copy_from_slice(&bytes);
        self.session_key = Some(key);
        Ok(())
    }

    /// 获取当前使用的密钥
    fn get_key(&self, use_auth_key: bool) -> &[u8; 32] {
        if use_auth_key {
            &self.auth_key
        } else {
            self.session_key.as_ref().unwrap_or(&self.auth_key)
        }
    }

    /// 加密数据
    pub fn encrypt(&self, data: &[u8], use_auth_key: bool) -> Result<Vec<u8>> {
        let key = self.get_key(use_auth_key);
        let iv = [0u8; 16]; // 零 IV

        // 计算需要的缓冲区大小 (包括 PKCS7 填充)
        let block_size = 16;
        let padded_len = ((data.len() / block_size) + 1) * block_size;
        let mut buffer = vec![0u8; padded_len];
        buffer[..data.len()].copy_from_slice(data);

        let cipher = Aes256CbcEnc::new(key.into(), &iv.into());
        let encrypted = cipher
            .encrypt_padded_mut::<Pkcs7>(&mut buffer, data.len())
            .map_err(|e| Mt4Error::Encryption(format!("Encryption failed: {:?}", e)))?;

        Ok(encrypted.to_vec())
    }

    /// 解密数据
    pub fn decrypt(&self, data: &[u8]) -> Result<Vec<u8>> {
        let key = self.session_key.as_ref().unwrap_or(&self.auth_key);
        let iv = [0u8; 16]; // 零 IV

        let mut buffer = data.to_vec();
        let cipher = Aes256CbcDec::new(key.into(), &iv.into());

        let decrypted = cipher
            .decrypt_padded_mut::<Pkcs7>(&mut buffer)
            .map_err(|e| Mt4Error::Decryption(format!("Decryption failed: {:?}", e)))?;

        Ok(decrypted.to_vec())
    }

    /// 获取认证密钥的十六进制表示
    pub fn auth_key_hex(&self) -> String {
        hex::encode(&self.auth_key)
    }

    /// 获取会话密钥的十六进制表示
    pub fn session_key_hex(&self) -> Option<String> {
        self.session_key.map(|k| hex::encode(&k))
    }
}

impl Default for Mt4Crypto {
    fn default() -> Self {
        Self::new().expect("Failed to initialize crypto")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_key_decode() {
        let crypto = Mt4Crypto::new().unwrap();
        assert_eq!(crypto.auth_key.len(), 32);
        assert_eq!(
            crypto.auth_key_hex(),
            "02de02a1a65cc794684fcbea1ecb0fd74ae657e43662c11eee885d2fd64f4964"
        );
    }

    #[test]
    fn test_encrypt_decrypt() {
        let crypto = Mt4Crypto::new().unwrap();
        let data = b"Hello, MT4!";

        let encrypted = crypto.encrypt(data, true).unwrap();
        assert_ne!(encrypted, data);

        // 解密需要使用相同的密钥
        let decrypted = crypto.decrypt(&encrypted).unwrap();
        assert_eq!(decrypted, data);
    }

    #[test]
    fn test_session_key() {
        let mut crypto = Mt4Crypto::new().unwrap();
        let session_key = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

        crypto.set_session_key(session_key).unwrap();
        assert!(crypto.session_key.is_some());
        assert_eq!(crypto.session_key_hex().unwrap(), session_key);
    }
}
