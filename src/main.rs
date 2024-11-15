use clap::{value_parser, Arg, Command}; // Added missing imports
use flate2::read::GzDecoder;
use serde::Serialize;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};

#[derive(Serialize, Debug)]
struct ChunkInfo {
    chunk_number: usize,
    offset: u64,
    compressed_size: u64,
    uncompressed_size: u64,
    compression_ratio: f64,
    header_info: String,
    uncompressed_data: Vec<u8>,
}

#[derive(Serialize, Debug)]
struct FileSummary {
    total_chunks: usize,
    total_compressed_size: u64,
    total_uncompressed_size: u64,
    average_compression_ratio: f64,
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
            .help("Preview the content of chunks")
            .value_parser(value_parser!(String)))
        .arg(Arg::new("encoding")
            .short('e')
            .long("encoding")
            .help("Encoding for preview (default: utf-8)")
            .value_parser(value_parser!(String))
            .default_value("utf-8"))
        .get_matches();

    let file_path = matches.get_one::<String>("file").unwrap();
    let output_format = matches.get_one::<String>("output_format").unwrap();
    let preview = matches.get_one::<String>("preview");
    let encoding = matches.get_one::<String>("encoding").unwrap();

    match inspect_file(file_path, output_format, preview.map(|s| s.as_str()), encoding) {
        Ok(_) => (),
        Err(e) => eprintln!("Error: {}", e),
    }
}

fn inspect_file(file_path: &str, output_format: &str, preview: Option<&str>, encoding: &str) -> io::Result<()> {
    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);
    let mut offset = 0;
    let mut chunk_number = 0;
    let mut total_compressed_size = 0;
    let mut total_uncompressed_size = 0;
    let mut chunks = Vec::new();

    loop {
        let chunk_info = match read_chunk(&mut reader, offset, chunk_number) {
            Ok(info) => info,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };

        total_compressed_size += chunk_info.compressed_size;
        total_uncompressed_size += chunk_info.uncompressed_size;
        offset += chunk_info.compressed_size;
        chunk_number += 1;
        chunks.push(chunk_info);
    }

    let summary = FileSummary {
        total_chunks: chunk_number,
        total_compressed_size,
        total_uncompressed_size,
        average_compression_ratio: total_uncompressed_size as f64 / total_compressed_size as f64,
    };

    if output_format == "json" {
        println!("{}", serde_json::to_string_pretty(&chunks)?);
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        for chunk in &chunks {
            println!("{:?}", chunk);
        }
        println!("{:?}", summary);
    }

    if let Some(preview) = preview {
        preview_chunks(file_path, preview, encoding)?;
    }

    Ok(())
}

fn read_chunk<R: Read + Seek>(reader: &mut R, offset: u64, chunk_number: usize) -> io::Result<ChunkInfo> {
    reader.seek(SeekFrom::Start(offset))?;
    
    // Read GZ header bytes
    let mut header = [0u8; 2];
    reader.read_exact(&mut header)?;
    if header != [0x1f, 0x8b] {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid gzip header"));
    }
    
    // Read compressed data into buffer
    let mut compressed_data = Vec::new();
    let mut limited_reader = reader.take(10 * 1024 * 1024); // Limit to 10MB per chunk
    limited_reader.read_to_end(&mut compressed_data)?;
    
    // Create decoder for just this chunk
    let mut decoder = GzDecoder::new(&compressed_data[..]);
    let mut uncompressed_data = Vec::new();
    let uncompressed_size = decoder.read_to_end(&mut uncompressed_data)?;
    
    Ok(ChunkInfo {
        chunk_number,
        offset,
        compressed_size: compressed_data.len() as u64,
        uncompressed_size: uncompressed_size as u64,
        compression_ratio: uncompressed_size as f64 / compressed_data.len() as f64,
        header_info: format!("{:x?}", &header),
        uncompressed_data,
    })
}

fn preview_chunks(file_path: &str, preview: &str, encoding: &str) -> io::Result<()> {
    let file = File::open(file_path)?;
    let mut reader = BufReader::new(file);
    let mut offset = 0;
    let mut chunk_number = 0;

    // Parse preview range
    let preview_range: Vec<_> = preview.split('-').collect();
    let start = preview_range[0].parse::<usize>().unwrap_or(0);
    let end = preview_range.get(1).map_or(usize::MAX, |&s| s.parse::<usize>().unwrap_or(usize::MAX));

    while chunk_number < end {
        let chunk_info = match read_chunk(&mut reader, offset, chunk_number) {
            Ok(info) => info,
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e),
        };

        if chunk_number >= start {
            if encoding == "utf-8" {
                println!("{}", String::from_utf8_lossy(&chunk_info.uncompressed_data));
            } else {
                // Handle other encodings if necessary
            }
        }

        offset += chunk_info.compressed_size;
        chunk_number += 1;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_inspect_file() {
        // Path to the known test file
        let file_path = "tests/test.gz";

        // Ensure the test file exists
        assert!(Path::new(file_path).exists(), "Test file does not exist");

        // Run the inspect_file function
        let result = inspect_file(file_path, "human", None, "utf-8");
        assert!(result.is_ok());
    }
}