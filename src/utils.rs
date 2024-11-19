use crate::models::{PreviewSettings, ChunkFilterSettings};


impl PreviewSettings {
    pub fn parse(preview_arg: Option<&str>) -> Option<Self> {
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

impl ChunkFilterSettings {
    pub fn parse(filter_arg: Option<&str>) -> Option<Self> {
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
}