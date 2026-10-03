use crate::Error;
use std::sync::atomic::{fence, AtomicU64, Ordering::SeqCst};

pub(crate) const READERS: usize = 40;
pub(crate) const HEADER_WORDS: usize = 3 + 3 * READERS;
#[cfg(feature = "native-skip-miri")]
pub(crate) const HEADER_BYTES: usize = HEADER_WORDS * 8;
const READ_POINTERS: usize = 3;
const READ_VALIDS: usize = READ_POINTERS + READERS;
const READ_UIDS: usize = READ_VALIDS + READERS;
const WRAP: u64 = u64::MAX;
const MAX_OFFSET: usize = u32::MAX as usize;

const fn offset_index(value: u32) -> usize {
    value as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Position {
    cycle: u32,
    offset: u32,
}

impl Position {
    fn unpack(value: u64) -> Self {
        let bytes = value.to_ne_bytes();
        Self {
            cycle: u32::from_ne_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            offset: u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        }
    }

    fn pack(self) -> u64 {
        (u64::from(self.cycle) << 32) | u64::from(self.offset)
    }

    fn wrapped(self) -> Self {
        Self {
            cycle: self.cycle.wrapping_add(1),
            offset: 0,
        }
    }
}

pub(crate) struct QueueMemory<'a> {
    words: &'a [AtomicU64],
    capacity: usize,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Reader {
    index: usize,
    uid: u64,
    conflate: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Read {
    Empty,
    Message(Vec<u8>),
    Evicted,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Ready {
    Empty,
    Message,
    Evicted,
}

fn align(value: usize) -> Result<usize, Error> {
    value
        .checked_add(7)
        .map(|value| value & !7)
        .ok_or(Error::Corrupt("message length overflow"))
}

impl<'a> QueueMemory<'a> {
    pub(crate) fn new(words: &'a [AtomicU64]) -> Result<Self, Error> {
        let capacity = words
            .len()
            .checked_sub(HEADER_WORDS)
            .and_then(|words| words.checked_mul(8))
            .filter(|capacity| *capacity >= 24 && *capacity < MAX_OFFSET)
            .ok_or(Error::Invalid("invalid queue memory size"))?;
        Ok(Self { words, capacity })
    }

    fn reader_count(&self) -> Result<usize, Error> {
        usize::try_from(self.words[0].load(SeqCst))
            .ok()
            .filter(|count| *count <= READERS)
            .ok_or(Error::Corrupt("reader count exceeds the header"))
    }

    fn position(&self, value: u64) -> Result<Position, Error> {
        let position = Position::unpack(value);
        let offset = usize::try_from(position.offset)
            .map_err(|_| Error::Corrupt("queue offset is not addressable"))?;
        if offset >= self.capacity || !offset.is_multiple_of(8) {
            return Err(Error::Corrupt("queue offset is outside an aligned slot"));
        }
        Ok(position)
    }

    fn data_word(&self, offset: u32) -> &AtomicU64 {
        &self.words[HEADER_WORDS + offset_index(offset) / 8]
    }

    pub(crate) fn initialize_publisher(&self, uid: u64) {
        self.words[2].store(uid, SeqCst);
        self.words[0].store(0, SeqCst);
        for index in 0..READERS {
            self.words[READ_VALIDS + index].store(0, SeqCst);
            self.words[READ_UIDS + index].store(0, SeqCst);
        }
    }

    pub(crate) fn register(
        &self,
        uid: u64,
        conflate: bool,
        mut notify: impl FnMut(u32),
    ) -> Result<Reader, Error> {
        loop {
            let count = self.reader_count()?;
            if count == READERS {
                self.words[0].store(0, SeqCst);
                for index in 0..READERS {
                    self.words[READ_VALIDS + index].store(0, SeqCst);
                    let old_uid = self.words[READ_UIDS + index].load(SeqCst);
                    self.words[READ_UIDS + index].store(0, SeqCst);
                    notify(Position::unpack(old_uid).offset);
                }
                continue;
            }
            let count_word =
                u64::try_from(count).map_err(|_| Error::Corrupt("reader count conversion"))?;
            if self.words[0]
                .compare_exchange(count_word, count_word + 1, SeqCst, SeqCst)
                .is_ok()
            {
                self.words[READ_VALIDS + count].store(0, SeqCst);
                self.words[READ_POINTERS + count].store(0, SeqCst);
                self.words[READ_UIDS + count].store(uid, SeqCst);
                let reader = Reader {
                    index: count,
                    uid,
                    conflate,
                };
                reader.reset(self)?;
                return Ok(reader);
            }
        }
    }

    pub(crate) fn send(
        &self,
        uid: u64,
        bytes: &[u8],
        mut notify: impl FnMut(u32),
    ) -> Result<(), Error> {
        if self.words[2].load(SeqCst) != uid {
            return Err(Error::PublisherReplaced);
        }
        let total = bytes
            .len()
            .checked_add(8)
            .ok_or(Error::Invalid("message length overflow"))
            .and_then(align)?;
        if bytes.is_empty() || total > self.capacity / 3 {
            return Err(Error::Invalid("message must fit one third of the queue"));
        }
        let count = self.reader_count()?;
        let mut write = self.position(self.words[1].load(SeqCst))?;
        let offset = offset_index(write.offset);
        if self.capacity - offset <= total + 8 {
            self.data_word(write.offset).store(WRAP, SeqCst);
            for index in 0..count {
                let read = Position::unpack(self.words[READ_POINTERS + index].load(SeqCst));
                if read.offset > write.offset && read.cycle != write.cycle {
                    self.words[READ_VALIDS + index].store(0, SeqCst);
                }
            }
            write = write.wrapped();
            self.words[1].store(write.pack(), SeqCst);
        }
        let end = usize::try_from(write.offset)
            .map_err(|_| Error::Corrupt("queue offset is not addressable"))?
            .checked_add(total)
            .filter(|end| *end < self.capacity)
            .ok_or(Error::Corrupt("message exceeds queue memory"))?;
        for index in 0..count {
            let read = Position::unpack(self.words[READ_POINTERS + index].load(SeqCst));
            if read.offset >= write.offset
                && offset_index(read.offset) < end
                && read.cycle != write.cycle
            {
                self.words[READ_VALIDS + index].store(0, SeqCst);
            }
        }
        let length =
            u64::try_from(bytes.len()).map_err(|_| Error::Invalid("message length conversion"))?;
        self.data_word(write.offset).store(length, SeqCst);
        let first_word = HEADER_WORDS + offset_index(write.offset) / 8 + 1;
        for (index, chunk) in bytes.chunks(8).enumerate() {
            let word = &self.words[first_word + index];
            let mut value = word.load(SeqCst).to_ne_bytes();
            value[..chunk.len()].copy_from_slice(chunk);
            word.store(u64::from_ne_bytes(value), SeqCst);
        }
        fence(SeqCst);
        write.offset = u32::try_from(end).map_err(|_| Error::Corrupt("write offset overflow"))?;
        self.words[1].store(write.pack(), SeqCst);
        for index in 0..count {
            notify(Position::unpack(self.words[READ_UIDS + index].load(SeqCst)).offset);
        }
        Ok(())
    }

    pub(crate) fn readers_caught_up(&self) -> Result<bool, Error> {
        let count = self.reader_count()?;
        for index in 0..count {
            if self.words[READ_VALIDS + index].load(SeqCst) != 0
                && self.words[1].load(SeqCst) != self.words[READ_POINTERS + index].load(SeqCst)
            {
                return Ok(false);
            }
        }
        Ok(count > 0)
    }
}

impl Reader {
    #[cfg(feature = "native-skip-miri")]
    pub(crate) fn conflated(self) -> bool {
        self.conflate
    }

    fn reset(&self, queue: &QueueMemory<'_>) -> Result<(), Error> {
        queue.words[READ_VALIDS + self.index].store(1, SeqCst);
        let write = queue.words[1].load(SeqCst);
        queue.position(write)?;
        queue.words[READ_POINTERS + self.index].store(write, SeqCst);
        Ok(())
    }

    fn current(&self, queue: &QueueMemory<'_>) -> bool {
        queue.words[READ_UIDS + self.index].load(SeqCst) == self.uid
    }

    fn valid(&self, queue: &QueueMemory<'_>) -> bool {
        queue.words[READ_VALIDS + self.index].load(SeqCst) != 0
    }

    pub(crate) fn ready(&self, queue: &QueueMemory<'_>) -> Result<Ready, Error> {
        loop {
            if !self.current(queue) {
                return Ok(Ready::Evicted);
            }
            if !self.valid(queue) {
                self.reset(queue)?;
                continue;
            }
            let read = queue.position(queue.words[READ_POINTERS + self.index].load(SeqCst))?;
            let write = queue.position(queue.words[1].load(SeqCst))?;
            return Ok(if read.offset != write.offset {
                Ready::Message
            } else {
                Ready::Empty
            });
        }
    }

    pub(crate) fn receive(&self, queue: &QueueMemory<'_>) -> Result<Read, Error> {
        loop {
            if !self.current(queue) {
                return Ok(Read::Evicted);
            }
            if !self.valid(queue) {
                self.reset(queue)?;
                continue;
            }
            let mut read = queue.position(queue.words[READ_POINTERS + self.index].load(SeqCst))?;
            let write = queue.position(queue.words[1].load(SeqCst))?;
            if read.offset == write.offset {
                return Ok(Read::Empty);
            }
            let size = queue.data_word(read.offset).load(SeqCst);
            if !self.valid(queue) {
                self.reset(queue)?;
                continue;
            }
            if size == WRAP {
                if read.offset == 0 {
                    return Err(Error::Corrupt("wrap tag at the start of the queue"));
                }
                queue.words[READ_POINTERS + self.index].store(read.wrapped().pack(), SeqCst);
                continue;
            }
            let size = usize::try_from(size)
                .ok()
                .filter(|size| *size > 0 && *size < queue.capacity)
                .ok_or(Error::Corrupt("invalid message length tag"))?;
            let start = offset_index(read.offset) + 8;
            let end = start
                .checked_add(size)
                .filter(|end| *end <= queue.capacity)
                .ok_or(Error::Corrupt("message extends beyond queue memory"))?;
            let next = align(end)?;
            let old_offset = read.offset;
            read.offset =
                u32::try_from(next).map_err(|_| Error::Corrupt("read offset overflow"))?;
            if self.conflate && read.offset != write.offset {
                queue.words[READ_POINTERS + self.index].store(read.pack(), SeqCst);
                continue;
            }
            let mut bytes = Vec::new();
            bytes.try_reserve_exact(size)?;
            fence(SeqCst);
            let first_word = HEADER_WORDS + offset_index(old_offset) / 8 + 1;
            for index in 0..size.div_ceil(8) {
                let value = queue.words[first_word + index].load(SeqCst).to_ne_bytes();
                let remaining = size - bytes.len();
                bytes.extend_from_slice(&value[..remaining.min(8)]);
            }
            fence(SeqCst);
            queue.words[READ_POINTERS + self.index].store(read.pack(), SeqCst);
            if !self.valid(queue) {
                self.reset(queue)?;
                continue;
            }
            return Ok(Read::Message(bytes));
        }
    }
}

#[cfg(test)]
#[path = "queue_core_tests.rs"]
mod tests;
