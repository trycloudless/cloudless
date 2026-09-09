use zstd::{decode_all, encode_all};

use crate::{model::base::AppResult, ports::Compressor};

//Suggested levels:
//Mobile: 1–3
//Wi-Fi + charging: 3–6

pub struct ZstdCompressor {
    level: i32,
}

impl ZstdCompressor {
    pub fn new(level: i32) -> Self {
        Self { level }
    }
}

impl Compressor for ZstdCompressor {
    fn compress(&self, data: &[u8]) -> AppResult<Vec<u8>> {
        Ok(encode_all(data, self.level)?)
    }

    fn decompress(&self, data: &[u8]) -> AppResult<Vec<u8>> {
        Ok(decode_all(data)?)
    }
}
