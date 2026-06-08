use std::io::Read;

use anyhow::{Context, Result};
use flate2::read::GzDecoder;

pub(crate) fn fetch_tile_bytes(url: &str) -> Result<Vec<u8>> {
    let response = ureq::get(url)
        .call()
        .with_context(|| format!("failed to fetch {url}"))?;
    let mut reader = response.into_reader();
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .with_context(|| format!("failed to read response body from {url}"))?;
    decode_tile_bytes(url, bytes)
}

fn decode_tile_bytes(label: &str, bytes: Vec<u8>) -> Result<Vec<u8>> {
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut decoder = GzDecoder::new(bytes.as_slice());
        let mut out = Vec::new();
        decoder
            .read_to_end(&mut out)
            .with_context(|| format!("failed to decompress gzip tile {label}"))?;
        Ok(out)
    } else {
        Ok(bytes)
    }
}
