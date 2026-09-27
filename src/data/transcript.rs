use memchr::{memchr, memmem, memrchr};

fn line_around(buf: &[u8], at: usize) -> (usize, usize) {
    let start = memrchr(b'\n', &buf[..at]).map_or(0, |i| i + 1);
    let end = memchr(b'\n', &buf[at..]).map_or(buf.len(), |i| at + i);
    (start, end)
}

fn as_line(bytes: &[u8]) -> Option<&str> {
    let bytes = bytes.strip_suffix(b"\r").unwrap_or(bytes);
    std::str::from_utf8(bytes).ok()
}

/// Lines of `buf` containing `needle`, in file order; lines that are not UTF-8 are skipped.
pub fn lines_containing<'a>(buf: &'a [u8], needle: &'a [u8]) -> impl Iterator<Item = &'a str> {
    let mut next = 0;
    memmem::find_iter(buf, needle).filter_map(move |at| {
        if at < next {
            return None;
        }
        let (start, end) = line_around(buf, at);
        next = end + 1;
        as_line(&buf[start..end])
    })
}

pub fn lines_containing_rev<'a>(buf: &'a [u8], needle: &'a [u8]) -> impl Iterator<Item = &'a str> {
    let mut limit = usize::MAX;
    memmem::rfind_iter(buf, needle).filter_map(move |at| {
        if at >= limit {
            return None;
        }
        let (start, end) = line_around(buf, at);
        limit = start;
        as_line(&buf[start..end])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUF: &[u8] = b"a x x\nb\r\nc x\r\n\xff x\nx";

    #[test]
    fn yields_each_matching_line_once_in_order() {
        let got: Vec<&str> = lines_containing(BUF, b"x").collect();
        assert_eq!(got, ["a x x", "c x", "x"]);
    }

    #[test]
    fn reverse_yields_the_same_lines_last_first() {
        let got: Vec<&str> = lines_containing_rev(BUF, b"x").collect();
        assert_eq!(got, ["x", "c x", "a x x"]);
    }

    #[test]
    fn no_match_and_empty_input_yield_nothing() {
        assert_eq!(lines_containing(BUF, b"zz").count(), 0);
        assert_eq!(lines_containing(b"", b"x").count(), 0);
        assert_eq!(lines_containing_rev(b"", b"x").count(), 0);
    }
}
