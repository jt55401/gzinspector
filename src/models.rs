use serde::Serialize;

#[derive(Serialize, Debug, Clone)]
pub struct ChunkInfo {
    pub chunk_number: usize,
    pub offset: u64,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub compression_ratio: f64,
    pub header_info: String,
    #[serde(skip)]
    pub preview_data: Option<Vec<u8>>,
}

#[derive(Debug, Serialize)]
pub struct GzipHeaderInfo {
    pub compression_method: String,
    pub flags: Vec<String>,
    pub mtime: String,
    pub extra_flags: String,
    pub os: String,
    pub extra_fields: Vec<(u16, Vec<u8>)>,
    pub filename: Option<String>,
    pub comment: Option<String>,
}

#[derive(Serialize, Debug)]
pub struct FileSummary {
    pub total_chunks: usize,
    pub total_compressed_size: u64,
    pub total_uncompressed_size: u64,
    pub average_compression_ratio: f64,
}

pub struct PreviewSettings {
    pub head_lines: usize,
    pub tail_lines: Option<usize>,
}

pub struct ChunkFilterSettings {
    pub head_chunks: usize,
    pub tail_chunks: Option<usize>,
}

pub struct TailBuffer {
    pub chunks: Vec<ChunkInfo>,
    pub capacity: usize,
    pub total_seen: usize,
}


impl TailBuffer {
    pub fn new(capacity: usize) -> Self {
        Self {
            chunks: Vec::with_capacity(capacity),
            capacity,
            total_seen: 0,
        }
    }

    pub fn add(&mut self, chunk: ChunkInfo) {
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

    pub fn should_buffer(&self, chunk_num: usize) -> bool {
        chunk_num >= self.total_seen.saturating_sub(self.capacity)
    }

    pub fn get_buffered(&self) -> Vec<&ChunkInfo> {
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
