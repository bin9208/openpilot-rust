use crate::Error;
use std::{
    marker::PhantomData,
    ptr::NonNull,
    sync::atomic::{
        AtomicU64, AtomicU8,
        Ordering::{Relaxed, SeqCst},
    },
};

/// Borrow a live, initialized, eight-byte-aligned shared queue allocation.
///
/// # Safety
/// `base..base+length` must remain mapped and initialized for the returned lifetime.
/// All concurrent accesses within this process must use aligned 64-bit atomics.
pub(crate) unsafe fn queue_words<'a>(
    base: NonNull<u8>,
    length: usize,
) -> Result<&'a [AtomicU64], Error> {
    if !base.as_ptr().cast::<u64>().is_aligned()
        || !length.is_multiple_of(8)
        || length > isize::MAX as usize
    {
        return Err(Error::Invalid("queue mapping is not word aligned"));
    }
    // SAFETY: the caller supplies initialized live memory for this lifetime;
    // alignment and length are checked above, and AtomicU64 permits interior mutation.
    Ok(unsafe { std::slice::from_raw_parts(base.as_ptr().cast::<AtomicU64>(), length / 8) })
}

pub(crate) struct ImageMemory<'a> {
    base: NonNull<u8>,
    data_length: usize,
    lifetime: PhantomData<&'a ()>,
}

impl<'a> ImageMemory<'a> {
    /// # Safety
    /// `base..base+mapped_length` must remain mapped and initialized for `'a`.
    /// Complete pixel words use aligned 64-bit atomics, including partial writes.
    /// Only the disjoint final pixel remainder uses byte atomics. The frame-ID
    /// tail uses a 64-bit atomic when aligned, otherwise eight byte atomics.
    pub(crate) unsafe fn new(
        base: NonNull<u8>,
        data_length: usize,
        mapped_length: usize,
    ) -> Result<Self, Error> {
        if data_length == 0
            || !base.as_ptr().cast::<u64>().is_aligned()
            || data_length
                .checked_add(8)
                .is_none_or(|end| end > mapped_length)
            || mapped_length > isize::MAX as usize
        {
            return Err(Error::Invalid("invalid image mapping length"));
        }
        Ok(Self {
            base,
            data_length,
            lifetime: PhantomData,
        })
    }

    fn byte(&self, offset: usize) -> &AtomicU8 {
        // SAFETY: callers select only the final partial pixel word or unaligned
        // frame-ID bytes. These ranges never overlap the complete atomic words.
        let pointer = unsafe { self.base.as_ptr().add(offset) };
        // SAFETY: every u8 bit pattern is valid and alignment is one. All concurrent
        // in-process accesses to these pixels/unaligned frame-ID bytes are atomic.
        unsafe { AtomicU8::from_ptr(pointer) }
    }

    fn pixel_word(&self, offset: usize) -> &AtomicU64 {
        // SAFETY: callers choose eight-byte multiples in the complete-word pixel
        // prefix; construction guarantees an aligned base and a live allocation.
        let pointer = unsafe { self.base.as_ptr().add(offset) }.cast::<u64>();
        // SAFETY: complete pixel words are accessed only with this atomic width,
        // including partial byte-range updates, and never overlap the byte tail.
        unsafe { AtomicU64::from_ptr(pointer) }
    }

    pub(crate) fn write(&self, offset: usize, bytes: &[u8]) -> Result<(), Error> {
        if offset > self.data_length || bytes.len() > self.data_length - offset {
            return Err(Error::Invalid("VisionIPC write exceeds buffer"));
        }
        let word_end = self.data_length / 8 * 8;
        let mut consumed = 0;
        while consumed < bytes.len() {
            let position = offset + consumed;
            if position >= word_end {
                self.byte(position).store(bytes[consumed], Relaxed);
                consumed += 1;
                continue;
            }
            let within_word = position % 8;
            let count = (8 - within_word).min(bytes.len() - consumed);
            let word = self.pixel_word(position - within_word);
            if count == 8 {
                let mut value = [0; 8];
                value.copy_from_slice(&bytes[consumed..consumed + 8]);
                word.store(u64::from_ne_bytes(value), Relaxed);
            } else {
                let mut previous = word.load(Relaxed);
                loop {
                    let mut value = previous.to_ne_bytes();
                    value[within_word..within_word + count]
                        .copy_from_slice(&bytes[consumed..consumed + count]);
                    match word.compare_exchange_weak(
                        previous,
                        u64::from_ne_bytes(value),
                        Relaxed,
                        Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(observed) => previous = observed,
                    }
                }
            }
            consumed += count;
        }
        Ok(())
    }

    pub(crate) fn copy_into(&self, destination: &mut [u8]) -> Result<(), Error> {
        if destination.len() != self.data_length {
            return Err(Error::Invalid(
                "VisionIPC copy requires exact buffer length",
            ));
        }
        let word_end = self.data_length / 8 * 8;
        for (index, chunk) in destination[..word_end].chunks_exact_mut(8).enumerate() {
            chunk.copy_from_slice(&self.pixel_word(index * 8).load(Relaxed).to_ne_bytes());
        }
        for (index, value) in destination[word_end..].iter_mut().enumerate() {
            *value = self.byte(word_end + index).load(Relaxed);
        }
        Ok(())
    }

    pub(crate) fn copy_samples(
        &self,
        offsets: impl Iterator<Item = usize> + Clone,
        destination: &mut [u8],
    ) -> Result<(), Error> {
        if offsets.clone().count() != destination.len()
            || offsets.clone().any(|offset| offset >= self.data_length)
        {
            return Err(Error::Invalid(
                "VisionIPC sample indices or destination length invalid",
            ));
        }
        let word_end = self.data_length / 8 * 8;
        let mut cached: Option<(usize, [u8; 8])> = None;
        let mut offsets = offsets;
        for value in destination {
            // An arbitrary caller's cloned iterator need not reproduce its first pass.
            // Recheck every index before invoking the existing unchecked atomic accessor.
            let offset = offsets
                .next()
                .filter(|offset| *offset < self.data_length)
                .ok_or(Error::Invalid("VisionIPC sample index outside buffer"))?;
            if offset >= word_end {
                *value = self.byte(offset).load(Relaxed);
            } else {
                let word_offset = offset / 8 * 8;
                let bytes = match cached {
                    Some((previous, bytes)) if previous == word_offset => bytes,
                    _ => {
                        let bytes = self.pixel_word(word_offset).load(Relaxed).to_ne_bytes();
                        cached = Some((word_offset, bytes));
                        bytes
                    }
                };
                *value = bytes[offset % 8];
            }
        }
        if offsets.next().is_some() {
            return Err(Error::Invalid("VisionIPC sample destination too short"));
        }
        Ok(())
    }

    fn aligned_frame_id(&self) -> Option<&AtomicU64> {
        // SAFETY: construction proves the eight-byte tail lies in this live mapping.
        let pointer = unsafe { self.base.as_ptr().add(self.data_length) }.cast::<u64>();
        if !pointer.is_aligned() {
            return None;
        }
        // SAFETY: alignment is checked; the tail is disjoint from all pixel accesses.
        // All in-process accesses to this aligned tail use the same atomic width.
        Some(unsafe { AtomicU64::from_ptr(pointer) })
    }

    pub(crate) fn frame_id(&self) -> u64 {
        if let Some(frame_id) = self.aligned_frame_id() {
            return frame_id.load(SeqCst);
        }
        let mut bytes = [0; 8];
        for (index, value) in bytes.iter_mut().enumerate() {
            *value = self.byte(self.data_length + index).load(SeqCst);
        }
        u64::from_ne_bytes(bytes)
    }

    pub(crate) fn set_frame_id(&self, value: u64) {
        if let Some(frame_id) = self.aligned_frame_id() {
            frame_id.store(value, SeqCst);
            return;
        }
        for (index, value) in value.to_ne_bytes().into_iter().enumerate() {
            self.byte(self.data_length + index).store(value, SeqCst);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::UnsafeCell;

    #[repr(align(8))]
    struct Storage<const N: usize>(UnsafeCell<[u8; N]>);

    impl<const N: usize> Storage<N> {
        fn new() -> Self {
            Self(UnsafeCell::new([0; N]))
        }

        fn pointer(&self) -> NonNull<u8> {
            NonNull::new(self.0.get().cast::<u8>()).unwrap()
        }
    }

    #[test]
    fn mapped_queue_words_preserve_alignment_and_shared_mutation() {
        let storage = Storage::<1024>::new();
        // SAFETY: aligned initialized UnsafeCell storage remains live for both
        // borrows, and the test uses only 64-bit atomic accesses to these bytes.
        let first = unsafe { queue_words(storage.pointer(), 1024) }.unwrap();
        // SAFETY: the same live storage is shared exclusively through atomic words.
        let second = unsafe { queue_words(storage.pointer(), 1024) }.unwrap();

        first[123].store(0x1020304050607080, SeqCst);

        assert_eq!(second[123].load(SeqCst), 0x1020304050607080);
    }

    #[test]
    fn mapped_image_access_handles_aligned_and_unaligned_frame_id_tails() {
        for length in [96, 98] {
            let storage = Storage::<112>::new();
            // SAFETY: initialized storage spans the requested pixels plus tail;
            // every access below follows ImageMemory's disjoint atomic widths.
            let memory = unsafe { ImageMemory::new(storage.pointer(), length, 112) }.unwrap();
            let input: Vec<u8> = (0..length)
                .map(|value| u8::try_from(value).unwrap())
                .collect();

            memory.write(0, &input).unwrap();
            memory.set_frame_id(0xff00aa5512345678);
            let mut copied = vec![0; length];
            memory.copy_into(&mut copied).unwrap();

            assert_eq!(copied, input);
            assert_eq!(memory.frame_id(), 0xff00aa5512345678);
            assert!(memory.write(length, &[1]).is_err());
            assert!(memory.write(usize::MAX, &[1]).is_err());
            assert!(memory.copy_into(&mut copied[..length - 1]).is_err());
        }
    }

    #[test]
    fn incomplete_image_tail_is_rejected_before_pointer_arithmetic() {
        let storage = Storage::<112>::new();

        // SAFETY: the allocation itself is valid; the deliberately invalid lengths
        // must be rejected before any pointer access or view is returned.
        let result = unsafe { ImageMemory::new(storage.pointer(), 108, 112) };

        assert!(result.is_err());
        let misaligned = storage
            .pointer()
            .map_addr(|address| address.checked_add(1).unwrap());
        // SAFETY: the shifted range still lies in live storage; construction must
        // reject its misalignment before creating an atomic-word reference.
        assert!(unsafe { ImageMemory::new(misaligned, 96, 104) }.is_err());
    }

    #[test]
    fn partial_pixel_writes_preserve_neighboring_words_and_unaligned_frame_tail() {
        let storage = Storage::<112>::new();
        // SAFETY: aligned storage remains live and every overlapping pixel access
        // follows the same complete-word or disjoint byte-tail atomic convention.
        let memory = unsafe { ImageMemory::new(storage.pointer(), 98, 112) }.unwrap();
        let mut expected = [0xa5; 98];
        memory.write(0, &expected).unwrap();
        memory.set_frame_id(0x8877665544332211);
        for offset in [0, 1, 7, 8, 9, 63, 64, 95, 96, 97, 98] {
            for length in [0, 1, 2, 7, 8, 9] {
                if offset + length > expected.len() {
                    continue;
                }
                let bytes = vec![(offset + length) as u8; length];
                expected[offset..offset + length].copy_from_slice(&bytes);

                memory.write(offset, &bytes).unwrap();
                let mut actual = [0; 98];
                memory.copy_into(&mut actual).unwrap();

                assert_eq!(actual, expected, "offset={offset} length={length}");
                assert_eq!(memory.frame_id(), 0x8877665544332211);
            }
        }
    }

    #[test]
    fn concurrent_partial_writes_to_one_pixel_word_keep_both_byte_ranges() {
        let words = [AtomicU64::new(0), AtomicU64::new(0)];
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            for (offset, value) in [(0, 0x11), (4, 0xaa)] {
                let words = &words;
                let barrier = &barrier;
                scope.spawn(move || {
                    let base = NonNull::new(words.as_ptr().cast_mut().cast::<u8>()).unwrap();
                    // SAFETY: the shared array consists entirely of initialized
                    // atomic words and outlives both joined threads; no byte access
                    // overlaps these complete eight-byte pixels or aligned tail.
                    let memory = unsafe { ImageMemory::new(base, 8, 16) }.unwrap();
                    barrier.wait();
                    memory.write(offset, &[value; 4]).unwrap();
                });
            }
        });
        assert_eq!(
            words[0].load(Relaxed).to_ne_bytes(),
            [0x11, 0x11, 0x11, 0x11, 0xaa, 0xaa, 0xaa, 0xaa]
        );
        assert_eq!(words[1].load(Relaxed), 0);
    }
    #[test]
    fn sampled_pixels_match_full_atomic_copy_including_partial_word_tails() {
        let storage = Storage::<112>::new();
        for length in 1..=104 {
            // SAFETY: aligned owned initialized storage remains live for this test;
            // ImageMemory validates the disjoint complete-word and byte-tail ranges.
            let memory = unsafe { ImageMemory::new(storage.pointer(), length, 112) }.unwrap();
            let bytes: Vec<_> = (0..length).map(|index| (index * 31) as u8).collect();
            memory.write(0, &bytes).unwrap();
            memory.set_frame_id(0x1122334455667788);
            for skip in [1, 2, 3, 8, 17] {
                let indices = (0..length).step_by(skip);
                let mut sampled = vec![0; indices.clone().count()];
                memory.copy_samples(indices.clone(), &mut sampled).unwrap();
                assert_eq!(
                    sampled,
                    indices.map(|offset| bytes[offset]).collect::<Vec<_>>()
                );
            }
            let mut sampled = [0xa5; 2];
            assert!(memory
                .copy_samples([0, length].into_iter(), &mut sampled)
                .is_err());
            assert_eq!(sampled, [0xa5; 2]);
            assert!(memory.copy_samples([0].into_iter(), &mut sampled).is_err());
            assert!(memory
                .copy_samples([usize::MAX, 0].into_iter(), &mut sampled)
                .is_err());
            assert_eq!(memory.frame_id(), 0x1122334455667788);
        }
    }
}
