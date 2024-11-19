use clap::{value_parser, Arg, Command};
use flate2::read::GzDecoder;
use serde::Serialize;
use std::fs::File;
use std::convert::TryInto;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::fmt;
use chrono::DateTime;
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Serialize, Debug, Clone)]
struct ChunkInfo {
    chunk_number: usize,
    offset: u64,
    compressed_size: u64,
    uncompressed_size: u64,
    compression_ratio: f64,
    header_info: String,
    #[serde(skip)]
    preview_data: Option<Vec<u8>>,
}

#[derive(Debug, Serialize)]
struct GzipHeaderInfo {
    compression_method: String,
    flags: Vec<String>,
    mtime: String,
    extra_flags: String,
    os: String,
    extra_fields: Vec<(u16, Vec<u8>)>,
    filename: Option<String>,
    comment: Option<String>,
}

impl fmt::Display for GzipHeaderInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}|{}", 
            self.compression_method,
            self.flags.join("|"))?;
        if let Some(fname) = &self.filename {
            write!(f, "|{}", fname)?;
        }
        Ok(())
    }
}

impl fmt::Display for ChunkInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ratio = if self.compression_ratio >= 1.0 {
            format!("🔓 {:.1}x", self.compression_ratio)
        } else {
            format!("🔒 {:.1}x", 1.0 / self.compression_ratio)
        };

        write!(f, "📦 #{:<5} │ 📍 {:<10} │ {} │ 📥 {:<8} │ 📤 {:<8} │ ℹ️  {}",
            self.chunk_number,
            self.offset,
            ratio,
            human_size(self.compressed_size),
            human_size(self.uncompressed_size),
            self.header_info)
    }
}

#[derive(Serialize, Debug)]
struct FileSummary {
    total_chunks: usize,
    total_compressed_size: u64,
    total_uncompressed_size: u64,
    average_compression_ratio: f64,
}

impl fmt::Display for FileSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\n📊 Summary:\n")?;
        write!(f, "├─ 📦 Chunks: {}\n", self.total_chunks)?;
        write!(f, "├─ 📥 Total Compressed: {}\n", human_size(self.total_compressed_size))?;
        write!(f, "├─ 📤 Total Uncompressed: {}\n", human_size(self.total_uncompressed_size))?;
        write!(f, "└─ 📈 Average Compression: {:.1}x", self.average_compression_ratio)
    }
}

fn human_size(size: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = size as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{:.0}{}", size, UNITS[unit_index])
    } else {
        format!("{:.1}{}", size, UNITS[unit_index])
    }
}

struct PreviewSettings {
    head_lines: usize,
    tail_lines: Option<usize>,
}

impl PreviewSettings {
    fn parse(preview_arg: Option<&str>) -> Option<Self> {
        preview_arg.map(|p| {
            let parts: Vec<&str> = p.split(':').collect();
            let head = parts[0].parse().unwrap_or(5);
            let tail = parts.get(1).and_then(|s| s.parse().ok());
            PreviewSettings {
                head_lines: head,
                tail_lines: tail,
            }
        })
    }
}

struct ChunkFilterSettings {
    head_chunks: usize,
    tail_chunks: Option<usize>,
}

impl ChunkFilterSettings {
    fn parse(filter_arg: Option<&str>) -> Option<Self> {
        filter_arg.map(|p| {
            let parts: Vec<&str> = p.split(':').collect();
            let head = parts[0].parse().unwrap_or(5);
            let tail = parts.get(1).and_then(|s| s.parse().ok());
            ChunkFilterSettings {
                head_chunks: head,
                tail_chunks: tail,
            }
        })
    }

    fn should_print_chunk(&self, chunk_num: usize, total_chunks: usize) -> bool {
        if chunk_num < self.head_chunks {
            return true;
        }
        if let Some(tail) = self.tail_chunks {
            if chunk_num >= total_chunks.saturating_sub(tail) {
                return true;
            }
        }
        false
    }
}

struct TailBuffer {
    chunks: Vec<ChunkInfo>,
    capacity: usize,
    total_seen: usize,
}

impl TailBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            chunks: Vec::with_capacity(capacity),
            capacity,
            total_seen: 0,
        }
    }

    fn add(&mut self, chunk: ChunkInfo) {
        self.total_seen += 1;
        if self.chunks.len() < self.capacity {
            self.chunks.push(chunk);
        } else {
            let idx = self.total_seen % self.capacity;
            if let Some(slot) = self.chunks.get_mut(idx) {
                *slot = chunk;
            }
        }
    }

    fn should_buffer(&self, chunk_num: usize) -> bool {
        chunk_num >= self.total_seen.saturating_sub(self.capacity)
    }

    fn get_buffered(&self) -> Vec<&ChunkInfo> {
        if self.total_seen <= self.capacity {
            self.chunks.iter().collect()
        } else {
            let start_idx = self.total_seen % self.capacity;
            let mut result = Vec::with_capacity(self.capacity);
            // First add the chunks from start_idx to end (older chunks)
            result.extend(&self.chunks[start_idx..]);
            // Then add the chunks from beginning to start_idx (newer chunks)
            result.extend(&self.chunks[..start_idx]);
            result
        }
    }
}

fn main() {
    let matches = Command::new("gz_inspector")
        .version("1.0")
        .author("Jason Grey <jason@jason-grey.com>")
        .about("Inspect gzip/zlib compressed files")
        .arg(Arg::new("file")
            .help("The gzip/zlib file to inspect")
            .required(true)
            .index(1))
        .arg(Arg::new("output_format")
            .short('o')
            .long("output-format")
            .help("Output format: human or json")
            .value_parser(["human", "json"])
            .default_value("human"))
        .arg(Arg::new("preview")
            .short('p')
            .long("preview")
            .help("Preview content (format: HEAD:TAIL, e.g. '5:3' shows first 5 and last 3 lines)")
            .value_parser(value_parser!(String)))
        .arg(Arg::new("encoding")
            .short('e')
            .long("encoding")
            .help("Encoding for preview (default: utf-8)")
            .value_parser(value_parser!(String))
            .default_value("utf-8"))
        .arg(Arg::new("chunks")
            .short('c')
            .long("chunks")
            .help("Filter chunks to display (format: HEAD:TAIL, e.g. '5:3' shows first 5 and last 3 chunks)")
            .value_parser(value_parser!(String)))
        .get_matches();

    let file_path = matches.get_one::<String>("file").unwrap();
    let output_format = matches.get_one::<String>("output_format").unwrap();
    let preview = matches.get_one::<String>("preview");
    let encoding = matches.get_one::<String>("encoding").unwrap();
    let chunks = matches.get_one::<String>("chunks");

    match inspect_file(file_path, output_format, preview.map(|s| s.as_str()), encoding, chunks.map(|s| s.as_str())) {
        Ok(_) => (),
        Err(e) => eprintln!("Error: {}", e),
    }
}

fn inspect_file(
    file_path: &str, 
    output_format: &str, 
    preview: Option<&str>, 
    encoding: &str,
    chunks: Option<&str>
) -> io::Result<()> {
    let file = File::open(file_path)?;
    let file_size = file.metadata()?.len();
    let mut reader = BufReader::new(file);
    
    // Create progress bar on stderr
    let progress = ProgressBar::new(file_size).with_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
            .unwrap()
            .progress_chars("#>-")
    );
    progress.set_draw_target(indicatif::ProgressDrawTarget::stderr());

    let mut offset = 0;
    let mut chunk_number = 0;
    let mut total_compressed_size = 0;
    let mut total_uncompressed_size = 0;
    let preview_settings = PreviewSettings::parse(preview);
    let chunk_filter = ChunkFilterSettings::parse(chunks);

    // Initialize tail buffer if needed
    let mut tail_buffer = chunk_filter.as_ref()
        .and_then(|f| f.tail_chunks)
        .map(|tail| TailBuffer::new(tail));

    loop {
        let chunk_info = match read_chunk(&mut reader, offset, chunk_number) {
            Ok(info) => info,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => {
                progress.finish_and_clear();
                return Err(e);
            }
        };

        // Update progress
        progress.set_position(offset);

        let should_print = chunk_filter.as_ref()
            .map(|f| {
                if chunk_number < f.head_chunks {
                    true
                } else if let Some(ref mut buffer) = tail_buffer {
                    if buffer.should_buffer(chunk_number) {
                        buffer.add(chunk_info.clone());
                        false
                    } else {
                        false
                    }
                } else {
                    true
                }
            })
            .unwrap_or(true);

        if should_print {
            if output_format == "json" {
                print!("{}", serde_json::to_string(&chunk_info)?);
                println!();
            } else {
                println!("{}", chunk_info);
                if let Some(settings) = &preview_settings {
                    if let Some(data) = &chunk_info.preview_data {
                        print_preview(data, settings, encoding);
                    }
                }
            }
        }

        total_compressed_size += chunk_info.compressed_size;
        total_uncompressed_size += chunk_info.uncompressed_size;
        offset += chunk_info.compressed_size;
        chunk_number += 1;
    }

    // Finish and clear progress bar
    progress.finish_and_clear();

    // Print buffered tail chunks
    if let Some(buffer) = tail_buffer {
        if chunk_number > buffer.capacity {
            if output_format == "human" {
                println!("          ...");
            }
        }
        for chunk in buffer.get_buffered() {
            if output_format == "json" {
                print!("{}", serde_json::to_string(chunk)?);
                println!();
            } else {
                println!("{}", chunk);
                if let Some(settings) = &preview_settings {
                    if let Some(data) = &chunk.preview_data {
                        print_preview(data, settings, encoding);
                    }
                }
            }
        }
    }

    // Print summary
    let summary = FileSummary {
        total_chunks: chunk_number,
        total_compressed_size,
        total_uncompressed_size,
        average_compression_ratio: total_uncompressed_size as f64 / total_compressed_size as f64,
    };

    if output_format == "json" {
        println!("{}", serde_json::to_string(&summary)?);
    } else {
        println!("{}", summary);
    }

    Ok(())
}

fn print_preview(data: &[u8], settings: &PreviewSettings, encoding: &str) {
    let text = String::from_utf8_lossy(data).into_owned();
    let lines: Vec<&str> = text.lines().collect();
    
    // Print head lines
    let head = settings.head_lines.min(lines.len());
    for (i, line) in lines[..head].iter().enumerate() {
        println!("     {:>4} │ {}", i + 1, line);
    }
    
    // Print tail lines if requested
    if let Some(tail_count) = settings.tail_lines {
        if head < lines.len() {
            println!("          | ...");
            let start = lines.len().saturating_sub(tail_count);
            for (i, line) in lines[start..].iter().enumerate() {
                println!("     {:>4} │ {}", start + i + 1, line);
            }
        }
    }
    println!("\n");
}

const GZIP_HEADER_SIZE: usize = 10;  // Standard GZIP header size
const GZIP_FOOTER_SIZE: usize = 8;   // CRC32 (4 bytes) + ISIZE (4 bytes)
const CRC32_SIZE: usize = 4;
const ISIZE_SIZE: usize = 4;

#[derive(Debug)]
struct GzipValidationError {
    claimed_size: u64,
    actual_size: u64,
    error_type: &'static str,
}

fn parse_gzip_header(header: &[u8], reader: &mut impl Read) -> io::Result<GzipHeaderInfo> {
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

fn validate_gzip_chunk(data: &[u8]) -> io::Result<(usize, u32)> {
    if data.len() < GZIP_HEADER_SIZE + GZIP_FOOTER_SIZE {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Chunk too small"));
    }

    // Check header magic
    if data[0] != 0x1f || data[1] != 0x8b {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid header magic"));
    }

    // Get the stored values from footer
    let footer_start = data.len() - GZIP_FOOTER_SIZE;
    let stored_crc32 = u32::from_le_bytes(data[footer_start..footer_start + 4].try_into().unwrap());
    let stored_size = u32::from_le_bytes(data[footer_start + 4..].try_into().unwrap());

    Ok((stored_size as usize, stored_crc32))
}

fn validate_member(data: &[u8]) -> bool {
    if data.len() < GZIP_HEADER_SIZE + GZIP_FOOTER_SIZE {
        return false;
    }
    
    // Check header magic
    if data[0] != 0x1f || data[1] != 0x8b || data[2] != 0x08 {
        return false;
    }
    
    // Try quick decompression to validate
    let mut decoder = GzDecoder::new(data);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf).is_ok()
}

fn is_complete_gzip_member(data: &[u8], is_final: bool) -> bool {
    if data.len() < GZIP_HEADER_SIZE + GZIP_FOOTER_SIZE {
        return false;
    }

    // Check magic numbers and compression method
    if data[0] != 0x1f || data[1] != 0x8b || data[2] != 0x08 {
        return false;
    }

    // For non-final chunks, be strict about validation
    if !is_final {
        let footer_start = data.len() - GZIP_FOOTER_SIZE;
        let stored_size = u32::from_le_bytes(data[footer_start + 4..].try_into().unwrap());
        let mut decoder = GzDecoder::new(data);
        let mut buf = Vec::with_capacity(stored_size as usize);
        return decoder.read_to_end(&mut buf).is_ok()
    }

    // For final chunk, just try to decompress what we have
    let mut decoder = GzDecoder::new(data);
    let mut buf = Vec::new();
    decoder.read_to_end(&mut buf).is_ok()
}

fn read_chunk<R: Read + Seek>(reader: &mut R, offset: u64, chunk_number: usize) -> io::Result<ChunkInfo> {
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

fn count_chunks(file_path: &str) -> io::Result<usize> {
    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);
    let mut offset = 0;
    let mut count = 0;

    loop {
        match read_chunk(&mut reader, offset, count) {
            Ok(info) => {
                offset += info.compressed_size;
                count += 1;
            }
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        }
    }

    Ok(count)
}

fn find_gzip_header(buffer: &[u8]) -> Option<usize> {
    for i in 0..buffer.len() - 1 {
        if buffer[i] == 0x1f && buffer[i + 1] == 0x8b {
            return Some(i);
        }
    }
    None
}
