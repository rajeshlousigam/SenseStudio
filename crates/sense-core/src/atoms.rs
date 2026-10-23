//! Top-level ISO-BMFF box scan. Only box headers are read, so this costs a
//! few hundred bytes of I/O even on multi-GB files.

use std::io::{self, Read, Seek, SeekFrom};

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BoxLayout {
    pub major_brand: Option<String>,
    /// A `moof` box exists: the file is fragmented (DASH/CMAF style).
    pub fragmented: bool,
    /// `None` when either `moov` or `mdat` is missing.
    pub moov_before_mdat: Option<bool>,
}

// Fragmented files have one moof+mdat pair per fragment; this bounds a corrupt file.
const MAX_BOXES: usize = 200_000;

pub fn scan<R: Read + Seek>(reader: &mut R) -> io::Result<BoxLayout> {
    let mut layout = BoxLayout::default();
    let mut first_moov: Option<usize> = None;
    let mut first_mdat: Option<usize> = None;
    let mut pos: u64 = 0;

    for index in 0..MAX_BOXES {
        // Once fragmentation and the moov/mdat order are known nothing else matters; stopping
        // here keeps remote checks to a handful of range requests on fragmented files.
        if layout.fragmented && first_moov.is_some() && first_mdat.is_some() {
            break;
        }
        reader.seek(SeekFrom::Start(pos))?;
        let mut header = [0u8; 8];
        if !read_full(reader, &mut header)? {
            break;
        }
        let size32 = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
        let kind = [header[4], header[5], header[6], header[7]];

        let (size, header_len) = match size32 {
            1 => {
                let mut large = [0u8; 8];
                if !read_full(reader, &mut large)? {
                    break;
                }
                (Some(u64::from_be_bytes(large)), 16)
            }
            0 => (None, 8), // box runs to end of file
            n => (Some(u64::from(n)), 8),
        };

        match &kind {
            b"ftyp" => {
                let mut brand = [0u8; 4];
                if read_full(reader, &mut brand)? {
                    layout.major_brand = Some(String::from_utf8_lossy(&brand).trim().to_owned());
                }
            }
            b"moov" => {
                first_moov.get_or_insert(index);
            }
            b"mdat" => {
                first_mdat.get_or_insert(index);
            }
            b"moof" => layout.fragmented = true,
            _ => {}
        }

        match size {
            Some(size) if size >= header_len => pos += size,
            _ => break,
        }
    }

    layout.moov_before_mdat = match (first_moov, first_mdat) {
        (Some(moov), Some(mdat)) => Some(moov < mdat),
        _ => None,
    };
    Ok(layout)
}

/// Fill `buf` completely; `Ok(false)` on a clean or truncated end of file.
fn read_full<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<bool> {
    let mut filled = 0;
    while filled < buf.len() {
        match reader.read(&mut buf[filled..]) {
            Ok(0) => return Ok(false),
            Ok(n) => filled += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn mp4_box(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn detects_faststart_layout() {
        let data = [mp4_box(b"ftyp", b"isom\0\0\0\0"), mp4_box(b"moov", &[0; 16]), mp4_box(b"mdat", &[0; 32])].concat();
        let layout = scan(&mut Cursor::new(data)).unwrap();
        assert_eq!(layout.major_brand.as_deref(), Some("isom"));
        assert_eq!(layout.moov_before_mdat, Some(true));
        assert!(!layout.fragmented);
    }

    #[test]
    fn detects_moov_at_end_and_fragments() {
        let data = [
            mp4_box(b"ftyp", b"dash\0\0\0\0"),
            mp4_box(b"mdat", &[0; 32]),
            mp4_box(b"moov", &[0; 16]),
            mp4_box(b"moof", &[0; 8]),
        ]
        .concat();
        let layout = scan(&mut Cursor::new(data)).unwrap();
        assert_eq!(layout.major_brand.as_deref(), Some("dash"));
        assert_eq!(layout.moov_before_mdat, Some(false));
        assert!(layout.fragmented);
    }

    #[test]
    fn stops_on_truncated_or_bogus_sizes() {
        let mut data = mp4_box(b"ftyp", b"isom\0\0\0\0");
        data.extend_from_slice(&[0, 0, 0, 4, b'b', b'a', b'd', b'!']); // size smaller than header
        let layout = scan(&mut Cursor::new(data)).unwrap();
        assert_eq!(layout.moov_before_mdat, None);
    }
}
