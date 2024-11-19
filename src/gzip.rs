use std::io::{self, Read, Seek, SeekFrom};
use flate2::read::GzDecoder;
use chrono::DateTime;
use crate::models::{ChunkInfo, GzipHeaderInfo};

const GZIP_HEADER_SIZE: usize = 10;  // Standard GZIP header size

pub fn parse_gzip_header(header: &[u8], reader: &mut impl Read) -> io::Result<GzipHeaderInfo> {
    let mut flags = Vec::new();
    if header[3] & 0x01 != 0 { flags.push("TEXT".to_string()); }
    if header[3] & 0x02 != 0 { flags.push("HCRC".to_string()); }
    if header[3] & 0x04 != 0 { flags.push("EXTRA".to_string()); }
    if header[3] & 0x08 != 0 { flags.push("NAME".to_string()); }
    if header[3] & 0x10 != 0 { flags.push("COMMENT".to_string()); }

    let mtime = u32::from_le_bytes(header[4..8].try_into().unwrap());
    let mtime_str = if mtime == 0 {
        "Not set".to_string()
    } else {
        DateTime::from_timestamp(mtime as i64, 0)
            .map_or("Invalid".to_string(), |dt| dt.to_string())
    };

    let extra_flags = match header[8] {
        2 => "max compression".to_string(),
        4 => "fastest".to_string(),
        _ => format!("unknown(0x{:02x})", header[8]),
    };

    let os = match header[9] {
        0 => "FAT".to_string(),
        1 => "Amiga".to_string(),
        2 => "VMS".to_string(),
        3 => "Unix".to_string(),
        4 => "VM/CMS".to_string(),
        5 => "Atari TOS".to_string(),
        6 => "HPFS".to_string(),
        7 => "Macintosh".to_string(),
        8 => "Z-System".to_string(),
        9 => "CP/M".to_string(),
        10 => "TOPS-20".to_string(),
        11 => "NTFS".to_string(),
        12 => "QDOS".to_string(),
        13 => "Acorn RISCOS".to_string(),
        255 => "unknown".to_string(),
        x => format!("unknown({})", x),
    };

    let mut extra_fields = Vec::new();
    let mut filename = None;
    let mut comment = None;

    // Read extra fields if present
    if header[3] & 0x04 != 0 {
        let mut xlen_bytes = [0u8; 2];
        reader.read_exact(&mut xlen_bytes)?;
        let xlen = u16::from_le_bytes(xlen_bytes);
        let mut extra = vec![0u8; xlen as usize];
        reader.read_exact(&mut extra)?;
        
        let mut pos = 0;
        while pos + 4 <= extra.len() {
            let si1 = extra[pos];
            let si2 = extra[pos + 1];
            let len = u16::from_le_bytes(extra[pos+2..pos+4].try_into().unwrap());
            let data = if pos + 4 + len as usize <= extra.len() {
                extra[pos+4..pos+4+len as usize].to_vec()
            } else {
                Vec::new()
            };
            extra_fields.push(((si1 as u16) << 8 | si2 as u16, data));
            pos += 4 + len as usize;
        }
    }

    // Read filename if present
    if header[3] & 0x08 != 0 {
        let mut fname = Vec::new();
        let mut buf = [0u8; 1];
        while reader.read_exact(&mut buf).is_ok() && buf[0] != 0 {
            fname.push(buf[0]);
        }
        filename = String::from_utf8(fname).ok();
    }

    // Read comment if present
    if header[3] & 0x10 != 0 {
        let mut comment_bytes = Vec::new();
        let mut buf = [0u8; 1];
        while reader.read_exact(&mut buf).is_ok() && buf[0] != 0 {
            comment_bytes.push(buf[0]);
        }
        comment = String::from_utf8(comment_bytes).ok();
    }

    Ok(GzipHeaderInfo {
        compression_method: match header[2] {
            8 => "deflate".to_string(),
            _ => format!("unknown({})", header[2]),
        },
        flags,
        mtime: mtime_str,
        extra_flags,
        os,
        extra_fields,
        filename,
        comment,
    })
}

pub fn read_chunk<R: Read + Seek>(reader: &mut R, offset: u64, chunk_number: usize) -> io::Result<ChunkInfo> {
    reader.seek(SeekFrom::Start(offset))?;
    
    // Read initial header
    let mut header = [0u8; GZIP_HEADER_SIZE];
    if reader.read_exact(&mut header).is_err() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "End of file"));
    }

    // Validate GZIP magic numbers
    if header[0] != 0x1f || header[1] != 0x8b {
        return Err(io::Error::new(io::ErrorKind::InvalidData, 
            format!("Invalid GZIP header: {:02x} {:02x} {:02x}", header[0], header[1], header[2])));
    }

    let header_info = parse_gzip_header(&header, reader)?;
    
    let mut compressed_data = Vec::with_capacity(8192);
    compressed_data.extend_from_slice(&header);
    
    let mut buffer = [0u8; 8192];
    let mut found_next = false;
    
    'read_loop: loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        // Look for next GZIP header
        for i in 0..bytes_read {
            if bytes_read - i >= 2 && buffer[i] == 0x1f && i + 1 < bytes_read && buffer[i + 1] == 0x8b {
                // Save current position
                let current_pos = reader.stream_position()?;
                
                // Try to validate current chunk up to this point
                let mut test_data = compressed_data.clone();
                test_data.extend_from_slice(&buffer[..i]);
                
                let mut decoder = GzDecoder::new(&test_data[..]);
                let mut test_buf = Vec::new();
                
                if decoder.read_to_end(&mut test_buf).is_ok() {
                    // Valid chunk found
                    compressed_data = test_data;
                    reader.seek(SeekFrom::Start(offset + compressed_data.len() as u64))?;
                    found_next = true;
                    break 'read_loop;
                }
                
                // If validation failed, restore position and continue
                reader.seek(SeekFrom::Start(current_pos))?;
            }
        }

        compressed_data.extend_from_slice(&buffer[..bytes_read]);
        
        // Safety limit with a more generous size for last chunk
        if compressed_data.len() > 20 * 1024 * 1024 {
            // Try to decompress what we have so far
            let mut decoder = GzDecoder::new(&compressed_data[..]);
            let mut test_buf = Vec::new();
            if decoder.read_to_end(&mut test_buf).is_ok() {
                break;
            }
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Chunk too large"));
        }
    }

    // Handle last chunk
    if !found_next {
        // Try to decompress full chunk first
        let mut decoder = GzDecoder::new(&compressed_data[..]);
        let mut test_buf = Vec::new();
        if decoder.read_to_end(&mut test_buf).is_err() {
            // If full decompression fails, try to find a valid ending
            for i in (GZIP_HEADER_SIZE..compressed_data.len()).rev() {
                let test_slice = &compressed_data[..i];
                let mut decoder = GzDecoder::new(test_slice);
                let mut test_buf = Vec::new();
                if decoder.read_to_end(&mut test_buf).is_ok() {
                    compressed_data.truncate(i);
                    break;
                }
            }
        }
    }

    // Final decompression attempt
    let mut decoder = GzDecoder::new(&compressed_data[..]);
    let mut decompressed = Vec::new();
    
    match decoder.read_to_end(&mut decompressed) {
        Ok(size) => Ok(ChunkInfo {
            chunk_number,
            offset,
            compressed_size: compressed_data.len() as u64,
            uncompressed_size: size as u64,
            compression_ratio: size as f64 / compressed_data.len() as f64,
            header_info: header_info.to_string(),
            preview_data: Some(decompressed),
        }),
        Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, 
            format!("Decompression error at offset {}: {}", offset, e)))
    }
}