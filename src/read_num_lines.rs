use std::io;
use std::io::{BufRead, ErrorKind};

// Read data from the reader until `num_lines` lines are reached and return the number of bytes.
// This is used to chunk the csv into multiple parts (each having `num_lines`) that can be
// parsed n parallel.
// Note: This is based on `read_until` in std::io.
pub fn read_num_lines<R: BufRead + ?Sized>(
    r: &mut R,
    num_lines: usize,
    buf: &mut Vec<u8>,
) -> io::Result<usize> {
    let mut read = 0;
    let mut iteration = 0;
    loop {
        // Fill the internal buffer. it should be configured with a big size - possibly
        // a size that allows to fit `num_lines` of csv data.
        let available = match r.fill_buf() {
            Ok(n) => n,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        // No more data, return the data we read so far.
        if available.is_empty() {
            return Ok(read);
        }

        // Loop until we parsed enough lines or until we need to fetch the next data into the buffer.
        let mut used = 0;
        loop {
            let (done, new_used) = {
                match memchr::memchr(b'\n', &available[used..]) {
                    Some(i) => {
                        buf.extend_from_slice(&available[used..=(used + i)]);
                        (true, i + 1)
                    }
                    None => {
                        buf.extend_from_slice(&available[used..]);
                        (false, available.len() - used)
                    }
                }
            };

            read += new_used;
            used += new_used;

            // If done, we found a full line
            if done {
                iteration += 1;
            } else {
                // Otherwise we need to fetch more data. Consume the data we read so far
                // so that the next call to `fill_buf` resumes correctly.
                r.consume(used);
                break;
            }

            // If we found enough lines, we return the number of bytes read.
            if iteration == num_lines {
                r.consume(used);
                return Ok(read);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bstr::ByteSlice;
    use std::io::BufReader;

    #[test]
    fn test_empty() {
        let buffer = "";
        let mut reader = BufReader::new(buffer.as_bytes());
        let mut buf = Vec::new();
        let result = read_num_lines(&mut reader, 1, &mut buf);
        assert_eq!(result.unwrap(), 0);
        assert_eq!(buf.as_bytes(), buffer.as_bytes());
    }

    #[test]
    fn test_new_line_at_end() {
        let buffer = "hello\n\n";
        let mut reader = BufReader::new(buffer.as_bytes());
        let mut buf = Vec::new();
        let result = read_num_lines(&mut reader, 2, &mut buf);
        assert_eq!(result.unwrap(), 7);
        assert_eq!(buf.as_bytes(), buffer.as_bytes());
    }

    #[test]
    fn test_read_num_lines() {
        let buffer = "Hello\nWorld";

        {
            let mut reader = BufReader::new(buffer.as_bytes());
            let mut buf = Vec::new();
            let result = read_num_lines(&mut reader, 1, &mut buf);
            assert_eq!(result.unwrap(), 6);
            assert_eq!(buf.as_bytes(), buffer[0..6].as_bytes());
            let mut buf2 = Vec::new();
            let result2 = read_num_lines(&mut reader, 1, &mut buf2);
            assert_eq!(result2.unwrap(), 5);
            assert_eq!(buf2.as_bytes(), buffer[6..11].as_bytes());
        }
    }

    #[test]
    fn test_read_two_iterations() {
        let buffer = "Hello\nWorld";
        let mut reader = BufReader::new(buffer.as_bytes());
        let mut buf = Vec::new();
        let result = read_num_lines(&mut reader, 2, &mut buf);
        assert_eq!(result.unwrap(), 11);
        assert_eq!(buf.as_bytes(), buffer.as_bytes());
    }

    #[test]
    fn test_small_buffer() {
        let buffer = "Hello\nWorld";
        let mut reader = BufReader::with_capacity(5, buffer.as_bytes());
        let mut buf = Vec::new();
        let result = read_num_lines(&mut reader, 2, &mut buf);
        assert_eq!(result.unwrap(), 11);
        assert_eq!(buf.as_bytes(), buffer.as_bytes());
    }
}
