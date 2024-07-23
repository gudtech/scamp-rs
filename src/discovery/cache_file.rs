use super::packet::AnnouncementPacket;
use anyhow::Result;
use std::io::{BufReader, Read};

/// An iterator which takes a buffered reader of bytes from the cache file and yields each announcement
/// Admittedly this is a little fancy versus just accumulating the whole file and splitting
/// but this is more fun :)
pub struct CacheFileAnnouncementIterator<R: Read> {
    reader: BufReader<R>,
}

static RECORD_DELIMITER: &str = "\n%%%\n";

impl<R: Read> CacheFileAnnouncementIterator<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }
}

impl<R: Read> Iterator for CacheFileAnnouncementIterator<R> {
    type Item = Result<AnnouncementPacket>;

    fn next(&mut self) -> Option<Self::Item> {
        use std::io::BufRead;

        let mut announcement_data = Vec::new();
        let delimiter_bytes = RECORD_DELIMITER.as_bytes();

        loop {
            let (consume, matched) = match self.reader.fill_buf() {
                Ok(buffer) if buffer.is_empty() => return None, // EOF
                Ok(buffer) => {
                    if let Some(pos) = buffer
                        .windows(delimiter_bytes.len())
                        .position(|window| window == delimiter_bytes)
                    {
                        announcement_data.extend_from_slice(&buffer[..pos]);
                        (pos + delimiter_bytes.len(), true)
                    } else {
                        announcement_data.extend_from_slice(buffer);

                        (buffer.len(), false)
                    }
                }
                Err(e) => return Some(Err(e.into())),
            };

            self.reader.consume(consume);

            if matched && announcement_data.len() > 0 {
                match String::from_utf8(announcement_data) {
                    Ok(data) => return Some(AnnouncementPacket::parse(&data).map_err(Into::into)),
                    Err(e) => return Some(Err(e.into())),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use super::*;

    #[test]
    fn test_cache_file_announcement_iterator() {
        let cache_file = File::open("samples/discovery-cache.txt").unwrap();
        let iterator = CacheFileAnnouncementIterator::new(cache_file);
        for announcement in iterator {
            println!("{:?}", announcement);
        }
    }
}
