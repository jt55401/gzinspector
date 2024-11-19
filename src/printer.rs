use crate::models::{PreviewSettings, ChunkInfo, FileSummary, GzipHeaderInfo};
use std::fmt;

pub fn print_preview(data: &[u8], settings: &PreviewSettings, _encoding: &str) {
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


pub fn human_size(size: u64) -> String {
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

impl fmt::Display for FileSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\n📊 Summary:\n")?;
        write!(f, "├─ 📦 Chunks: {}\n", self.total_chunks)?;
        write!(f, "├─ 📥 Total Compressed: {}\n", human_size(self.total_compressed_size))?;
        write!(f, "├─ 📤 Total Uncompressed: {}\n", human_size(self.total_uncompressed_size))?;
        write!(f, "└─ 📈 Average Compression: {:.1}x", self.average_compression_ratio)
    }
}

