//! Join a received record without copying a single-part ZMQ message.
pub fn join(parts: Vec<Vec<u8>>) -> Vec<u8> {
    let mut parts = parts.into_iter();
    let mut bytes = parts.next().unwrap_or_default();
    for part in parts {
        bytes.extend_from_slice(&part);
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::join;

    #[test]
    fn ordinary_record_keeps_the_received_allocation() {
        let input = vec![20, b'h', b'e', b'l', b'l', b'o'];
        let pointer = input.as_ptr();
        let capacity = input.capacity();
        let result = join(vec![input]);
        assert_eq!(result, b"\x14hello");
        assert_eq!(result.as_ptr(), pointer);
        assert_eq!(result.capacity(), capacity);
    }

    #[test]
    fn multipart_bytes_and_empty_record_contract_are_preserved() {
        for parts in [
            vec![],
            vec![vec![]],
            vec![vec![], vec![40], b"body".to_vec(), vec![]],
            vec![vec![20, 0xff], vec![0, 0xfe]],
            vec![b"a".to_vec(), b"bc".to_vec(), b"d".to_vec()],
        ] {
            let expected: Vec<_> = parts.iter().flatten().copied().collect();
            assert_eq!(join(parts), expected);
        }
    }
}
