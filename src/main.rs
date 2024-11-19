use clap::{value_parser, Arg, Command};
use std::fs::File;
use std::io::{self, BufReader};
use indicatif::{ProgressBar, ProgressStyle};
use gz_inspector::*;

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