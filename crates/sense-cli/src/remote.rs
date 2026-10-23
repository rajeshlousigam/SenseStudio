//! HTTP range reads for checking links (S3, CloudFront, any server that honours
//! `Range`) without downloading the file. Only box headers are read, so a check
//! costs a few small requests even for multi-GB videos.

use std::io::{self, Read, Seek, SeekFrom};

/// Each request fetches this much; MP4 box headers sit far apart, so bigger buys nothing.
const CHUNK: u64 = 64 * 1024;

pub struct HttpRangeReader<'a> {
    agent: &'a ureq::Agent,
    url: String,
    len: u64,
    pos: u64,
    cache_start: u64,
    cache: Vec<u8>,
}

impl<'a> HttpRangeReader<'a> {
    /// Fails early when the server does not support range requests, rather than
    /// silently streaming the whole file.
    pub fn open(agent: &'a ureq::Agent, url: &str) -> Result<Self, String> {
        let mut reader = Self { agent, url: url.to_owned(), len: 0, pos: 0, cache_start: 0, cache: Vec::new() };
        reader.fetch(0).map_err(|e| e.to_string())?;
        Ok(reader)
    }

    pub fn len(&self) -> u64 {
        self.len
    }

    fn fetch(&mut self, start: u64) -> io::Result<()> {
        let end = start + CHUNK - 1;
        let response = self
            .agent
            .get(&self.url)
            .set("Range", &format!("bytes={start}-{end}"))
            .call()
            .map_err(|e| io::Error::other(format!("request failed: {e}")))?;
        if response.status() != 206 {
            return Err(io::Error::other(format!(
                "server does not support range requests (HTTP {}); cannot check without downloading",
                response.status()
            )));
        }
        // Content-Range: bytes 0-65535/1714776400
        if let Some(total) = response
            .header("Content-Range")
            .and_then(|v| v.rsplit('/').next())
            .and_then(|t| t.trim().parse::<u64>().ok())
        {
            self.len = total;
        }
        let mut body = Vec::with_capacity(CHUNK as usize);
        response.into_reader().take(CHUNK).read_to_end(&mut body)?;
        self.cache_start = start;
        self.cache = body;
        Ok(())
    }
}

impl Read for HttpRangeReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.pos >= self.len || buf.is_empty() {
            return Ok(0);
        }
        let cached = self.pos >= self.cache_start && self.pos < self.cache_start + self.cache.len() as u64;
        if !cached {
            self.fetch(self.pos)?;
            if self.cache.is_empty() {
                return Ok(0);
            }
        }
        let offset = (self.pos - self.cache_start) as usize;
        let n = buf.len().min(self.cache.len() - offset);
        buf[..n].copy_from_slice(&self.cache[offset..offset + n]);
        self.pos += n as u64;
        Ok(n)
    }
}

impl Seek for HttpRangeReader<'_> {
    fn seek(&mut self, from: SeekFrom) -> io::Result<u64> {
        let target = match from {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
        };
        if target < 0 {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "seek before start"));
        }
        // Seeking past the end is allowed (like files); reads there return 0.
        self.pos = target as u64;
        Ok(self.pos)
    }
}
