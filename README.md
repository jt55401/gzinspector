# GZInspector

A robust command-line tool for inspecting and analyzing GZIP/ZLIB compressed files. GZInspector provides detailed information about compression chunks, headers, and content previews with support for both human-readable and JSON output formats.

## Motivation

Most GZIP implementations discard chunk boundaries during decompression since they're typically irrelevant for the decompressed output. However, certain file formats leverage GZIP chunks as a core feature, allowing selective decompression of individual chunks when their byte offsets and lengths are known.

This chunked compression approach is particularly prevalent in web archiving formats, including:
- [WARC, WET, WAT](https://commoncrawl.org/blog/web-archiving-file-formats-explained) files used by web archives to store crawled content
- [CDX/J and ZipNum encoded CDX](https://commoncrawl.org/blog/announcing-the-common-crawl-index) files that enable efficient index lookups 

These formats are actively used by major web archiving initiatives like [CommonCrawl](https://commoncrawl.org/) and the [Internet Archive](https://archive.org/) to manage and provide access to petabyte-scale web archives.

## Features

- 📦 Chunk-by-chunk analysis of GZIP files
- 📊 Detailed compression statistics and ratios
- 🔍 Content preview capabilities
- 🎯 Support for concatenated GZIP files
- 💾 Multiple output formats (human-readable and JSON)
- 📝 Comprehensive header information including timestamps and flags
- 🔄 Automatic encoding detection and handling

## Installation

### Using Rust Cargo

```cargo install gzinspector```

### Pre-built Binary (Linux)

To install the pre-built binary for Linux:

```bash
# Download the binary
# Download latest release from:
# https://github.com/jt55401/gzinspector/releases/latest
wget $(curl -s https://api.github.com/repos/jt55401/gzinspector/releases/latest | grep "browser_download_url.*tar\.gz" | cut -d '"' -f 4)

# Or browse all releases at:
# https://github.com/jt55401/gzinspector/releases

# Extract the binary
tar -xzf gzinspector-linux-x86_64.tar.gz

# Move the binary to a directory in your PATH
sudo mv gzinspector /usr/local/bin/
```

### From Source

To install GZInspector from source, you'll need Rust and Cargo installed on your system. Then:

```bash
# Clone the repository
git clone https://github.com/jt55401/gzinspector.git

# Build the project
cd gzinspector
cargo build --release

# The binary will be available at target/release/gzinspector
```

## Usage

```bash
gzinspector [OPTIONS] <FILE>
```

### Options

- `-o, --output-format <FORMAT>`: Output format (human or json) [default: human]
- `-p, --preview <PREVIEW>`: Preview content (format: HEAD:TAIL, e.g. '5:3' shows first 5 and last 3 lines)
- `-c, --chunks <CHUNKS>`: Only show first and last N chunks (format: HEAD:TAIL, e.g. '5:3' shows first 5 and last 3)
- `-e, --encoding <ENCODING>`: Encoding for preview [default: utf-8]
- `-h, --help`: Display help information
- `-V, --version`: Display version information

### Examples

Basic file inspection:
```bash
gzinspector example.gz
```

Show JSON output:
```bash
gzinspector -o json example.gz
```

Preview content (first 5 lines and last 3 lines):
```bash
gzinspector -p 5:3 example.gz
```

## Output Format

### Human-readable Output

The human-readable output includes:

```
📦 #1    │ 📍 0         │ 🔓 2.5x │ 📥 1.2KB   │ 📤 3.0KB   │ ℹ️  deflate|EXTRA|NAME|example.txt
```

Where:
- 📦 #N: Chunk number
- 📍: Offset in file
- 🔓/🔒: Compression ratio (with direction indicator)
- 📥: Compressed size
- 📤: Uncompressed size
- ℹ️: Header information

### JSON Output

JSON output provides detailed information in a machine-readable format:

```json
{
  "chunk_number": 1,
  "offset": 0,
  "compressed_size": 1234,
  "uncompressed_size": 3000,
  "compression_ratio": 2.43,
  "header_info": "deflate|EXTRA|NAME|example.txt"
}
```

## File Summary

Both output formats include a summary showing:
- Total number of chunks
- Total compressed size
- Total uncompressed size
- Average compression ratio

## Dependencies

- `flate2`: GZIP/ZLIB compression library
- `serde`: Serialization framework
- `clap`: Command line argument parsing
- `chrono`: Date and time functionality
- `crc32fast`: CRC32 checksum calculation

## Building from Source

1. Ensure you have Rust installed (1.56.0 or later)
2. Clone the repository
3. Run `cargo build --release`

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under the MIT License - see the LICENSE file for details.

## Author

Jason Grey (jason@jason-grey.com)

## Version History

- 0.1.0: Initial release
  - Basic GZIP file inspection
  - Human-readable and JSON output formats
  - Content preview functionality

- 0.2.0: Chunks release
  - Ability to show first N and last N chunks of the file
  - Shows progress bar during tail scan of large files