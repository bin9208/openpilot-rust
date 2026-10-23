//! Source-first private-PTY/reader cases. UNRUN; never use stdin/env/current TTY.
use super::*;
use rustix::termios::{self, LocalModes, OptionalActions, SpecialCodeIndex};
use std::{
    fs::File,
    io::{Cursor, Write},
    os::fd::AsFd,
};

fn mode(fd: BorrowedFd<'_>) -> Termios {
    termios::tcgetattr(fd).unwrap()
}
fn set(fd: BorrowedFd<'_>, value: &Termios) {
    termios::tcsetattr(fd, OptionalActions::Flush, value).unwrap();
}
fn saved(stack: &Mutex<ExitStack>, state: Arc<State>) -> io::Result<()> {
    stack.lock().unwrap().entries.push(state);
    Ok(())
}
fn fixture() -> (File, OwnedFd, Termios) {
    let (master, slave) = system::private_pty().unwrap();
    let mut original = mode(slave.as_fd());
    original
        .local_modes
        .insert(LocalModes::ICANON | LocalModes::ECHO);
    set(slave.as_fd(), &original);
    (master, slave, original)
}
fn same(actual: &Termios, expected: &Termios) {
    assert_eq!(actual.input_modes, expected.input_modes);
    assert_eq!(actual.output_modes, expected.output_modes);
    assert_eq!(actual.control_modes, expected.control_modes);
    assert_eq!(actual.local_modes, expected.local_modes);
    assert_eq!(
        actual.special_codes[SpecialCodeIndex::VMIN],
        expected.special_codes[SpecialCodeIndex::VMIN]
    );
    assert_eq!(
        actual.special_codes[SpecialCodeIndex::VTIME],
        expected.special_codes[SpecialCodeIndex::VTIME]
    );
}

#[test]
fn constructor_saves_and_changes_only_canonical_echo_without_nonblock() {
    let (_master, slave, original) = fixture();
    let flags = rustix::fs::fcntl_getfl(&slave).unwrap();
    let stack = Mutex::new(ExitStack::default());
    let kb = KBHit::create(slave, |s| saved(&stack, s)).unwrap();
    same(&kb.old_term().unwrap(), &original);
    let mut expected = original.clone();
    expected
        .local_modes
        .remove(LocalModes::ICANON | LocalModes::ECHO);
    same(&kb.new_term().unwrap(), &expected);
    same(&mode(kb.stdin_fd()), &expected);
    assert_eq!(rustix::fs::fcntl_getfl(kb.stdin_fd()).unwrap(), flags);
    assert_eq!(stack.lock().unwrap().entries.len(), 1);
    kb.set_normal_term().unwrap();
    same(&mode(kb.stdin_fd()), &original);
}

#[test]
fn explicit_reset_does_not_unregister_and_repeated_set_keeps_latest_old() {
    let (_master, slave, original) = fixture();
    let stack = Mutex::new(ExitStack::default());
    let kb = KBHit::create(slave, |s| saved(&stack, s)).unwrap();
    let first_new = kb.new_term().unwrap();
    kb.set_kbhit_with(|s| saved(&stack, s)).unwrap();
    same(&kb.old_term().unwrap(), &first_new);
    assert_eq!(stack.lock().unwrap().entries.len(), 2);
    kb.set_normal_term().unwrap();
    same(&mode(kb.stdin_fd()), &first_new);
    restore_next(&stack).unwrap();
    restore_next(&stack).unwrap();
    same(&mode(kb.stdin_fd()), &first_new);
    set(kb.stdin_fd(), &original);
}

#[test]
fn callback_queue_is_lifo_across_objects_and_retains_dropped_handle() {
    let (_master, slave, original) = fixture();
    let inspection = rustix::io::dup(&slave).unwrap();
    let second = rustix::io::dup(&slave).unwrap();
    let stack = Mutex::new(ExitStack::default());
    let first = KBHit::create(slave, |s| saved(&stack, s)).unwrap();
    let noncanonical = mode(first.stdin_fd());
    let second = KBHit::create(second, |s| saved(&stack, s)).unwrap();
    drop(first);
    drop(second);
    same(&mode(inspection.as_fd()), &noncanonical); // no Drop reset
    restore_next(&stack).unwrap();
    same(&mode(inspection.as_fd()), &noncanonical);
    restore_next(&stack).unwrap();
    same(&mode(inspection.as_fd()), &original);
    restore_next(&stack).unwrap(); // no remaining registration
}

#[test]
fn every_old_registration_uses_latest_snapshot_of_same_object() {
    let (_master, slave, original) = fixture();
    let stack = Mutex::new(ExitStack::default());
    let kb = KBHit::create(slave, |s| saved(&stack, s)).unwrap();
    kb.set_normal_term().unwrap();
    let mut changed = original.clone();
    changed.local_modes.remove(LocalModes::ECHO);
    set(kb.stdin_fd(), &changed);
    kb.set_kbhit_with(|s| saved(&stack, s)).unwrap();
    same(&kb.old_term().unwrap(), &changed);
    restore_next(&stack).unwrap();
    same(&mode(kb.stdin_fd()), &changed);
    set(kb.stdin_fd(), &original);
    restore_next(&stack).unwrap();
    same(&mode(kb.stdin_fd()), &changed);
    set(kb.stdin_fd(), &original);
}

#[test]
fn registration_failure_is_after_terminal_change_without_rollback() {
    let (_master, slave, original) = fixture();
    let inspection = rustix::io::dup(&slave).unwrap();
    assert!(KBHit::create(slave, |_| Err(io::Error::other(
        "synthetic registrar failure"
    )))
    .is_err());
    assert!(!mode(inspection.as_fd())
        .local_modes
        .contains(LocalModes::ICANON | LocalModes::ECHO));
    set(inspection.as_fd(), &original);
}

#[test]
fn nonterminal_constructor_error_never_registers() {
    let (reader, _writer) = std::os::unix::net::UnixStream::pair().unwrap();
    let mut called = false;
    let result = KBHit::create(reader.into(), |_| {
        called = true;
        Ok(())
    });
    assert!(result.is_err());
    assert!(!called);
}

#[test]
fn set_flushes_pending_private_pty_input_and_select_checks_readiness() {
    let (mut master, slave, _original) = fixture();
    let stack = Mutex::new(ExitStack::default());
    let kb = KBHit::create(slave, |s| saved(&stack, s)).unwrap();
    master.write_all(b"x").unwrap();
    system::wait_readable(kb.stdin_fd()).unwrap();
    assert!(KBHit::kbhit(kb.stdin_fd()).unwrap());
    kb.set_kbhit_with(|s| saved(&stack, s)).unwrap();
    assert!(!KBHit::kbhit(kb.stdin_fd()).unwrap());
    kb.set_normal_term().unwrap();
}

#[test]
fn normal_reset_flushes_pending_private_pty_input() {
    let (mut master, slave, original) = fixture();
    let stack = Mutex::new(ExitStack::default());
    let kb = KBHit::create(slave, |s| saved(&stack, s)).unwrap();
    master.write_all(b"z").unwrap();
    system::wait_readable(kb.stdin_fd()).unwrap();
    kb.set_normal_term().unwrap();
    assert!(!KBHit::kbhit(kb.stdin_fd()).unwrap());
    same(&mode(kb.stdin_fd()), &original);
}

#[test]
fn getch_counts_utf8_scalars_and_eof_without_implicit_stream() {
    let mut input = Cursor::new("aé한😀".as_bytes());
    for ch in ['a', 'é', '한', '😀'] {
        assert_eq!(KBHit::getch(&mut input).unwrap(), Some(ch));
    }
    assert_eq!(KBHit::getch(&mut input).unwrap(), None);
    assert_eq!(KBHit::getch(&mut input).unwrap(), None);
}

#[test]
fn getarrow_reads_three_characters_and_ignores_first_two_values() {
    for (text, expected) in [("\x1b[A!", 0), ("xxC!", 1), ("é😀B!", 2), ("12D!", 3)] {
        let mut input = Cursor::new(text.as_bytes());
        assert_eq!(KBHit::getarrow(&mut input).unwrap(), expected);
        assert_eq!(KBHit::getch(&mut input).unwrap(), Some('!'));
    }
}

#[test]
fn getarrow_short_read_is_index_error_and_unknown_third_is_value_error() {
    for (text, read) in [("", 0), ("a", 1), ("é😀", 2)] {
        assert!(
            matches!(KBHit::getarrow(&mut Cursor::new(text.as_bytes())), Err(ArrowError::IndexError { read: n }) if n == read)
        );
    }
    let mut input = Cursor::new(b"abZtail");
    assert!(matches!(
        KBHit::getarrow(&mut input),
        Err(ArrowError::ValueError('Z'))
    ));
    assert_eq!(KBHit::getch(&mut input).unwrap(), Some('t'));
}

#[test]
fn invalid_and_truncated_utf8_are_read_errors_not_arrow_index_errors() {
    for bytes in [
        &b"\xff"[..],
        &b"\xc0\x80"[..],
        &b"\xed\xa0\x80"[..],
        &b"\xf4\x90\x80\x80"[..],
        &b"\xe2\x82"[..],
    ] {
        assert_eq!(
            KBHit::getch(&mut Cursor::new(bytes)).unwrap_err().kind(),
            io::ErrorKind::InvalidData
        );
    }
    assert!(matches!(
        KBHit::getarrow(&mut Cursor::new(b"ab\xff")),
        Err(ArrowError::Read(_))
    ));
}

#[test]
fn interrupted_reads_retry_but_other_io_errors_propagate() {
    struct InterruptedOnce(bool);
    impl io::Read for InterruptedOnce {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if !self.0 {
                self.0 = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            out[0] = b'K';
            Ok(1)
        }
    }
    struct Broken;
    impl io::Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::ErrorKind::PermissionDenied.into())
        }
    }
    assert_eq!(
        KBHit::getch(&mut InterruptedOnce(false)).unwrap(),
        Some('K')
    );
    assert_eq!(
        KBHit::getch(&mut Broken).unwrap_err().kind(),
        io::ErrorKind::PermissionDenied
    );
}

#[test]
fn select_reports_eof_as_readable_for_supplied_nonterminal_stream() {
    let (reader, writer) = std::os::unix::net::UnixStream::pair().unwrap();
    assert!(!KBHit::kbhit(reader.as_fd()).unwrap());
    drop(writer);
    assert!(KBHit::kbhit(reader.as_fd()).unwrap());
}

#[test]
fn high_descriptor_range_is_rejected_before_fd_set() {
    assert_eq!(
        system::check_fd(nix::libc::FD_SETSIZE as i32)
            .unwrap_err()
            .kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        system::check_fd(-1).unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
}

// Supplemental source-review cases; these were added after the initial15-case snapshot.
#[test]
fn constructor_flushes_a_preexisting_private_canonical_line() {
    let (mut master, slave, _original) = fixture();
    master.write_all(b"old line\n").unwrap();
    system::wait_readable(slave.as_fd()).unwrap();
    let stack = Mutex::new(ExitStack::default());
    let kb = KBHit::create(slave, |s| saved(&stack, s)).unwrap();
    assert!(!KBHit::kbhit(kb.stdin_fd()).unwrap());
    kb.set_normal_term().unwrap();
}

#[test]
fn explicit_private_pty_reader_supplies_utf8_and_arrow_characters() {
    let (mut master, slave, _original) = fixture();
    let stack = Mutex::new(ExitStack::default());
    let kb = KBHit::create(slave, |s| saved(&stack, s)).unwrap();
    let mut reader = File::from(rustix::io::dup(kb.stdin_fd()).unwrap());
    master.write_all("é한😀\x1b[D".as_bytes()).unwrap();
    system::wait_readable(kb.stdin_fd()).unwrap();
    for ch in ['é', '한', '😀'] {
        assert_eq!(KBHit::getch(&mut reader).unwrap(), Some(ch));
    }
    assert_eq!(KBHit::getarrow(&mut reader).unwrap(), 3);
    assert!(!KBHit::kbhit(kb.stdin_fd()).unwrap());
    kb.set_normal_term().unwrap();
}

#[test]
fn private_exit_queue_error_does_not_discard_later_registered_reset() {
    let (_master, slave, original) = fixture();
    let stack = Mutex::new(ExitStack::default());
    let kb = KBHit::create(slave, |s| saved(&stack, s)).unwrap();
    let (not_terminal, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
    stack.lock().unwrap().entries.push(Arc::new(State {
        fd: not_terminal.into(),
        settings: Mutex::new(Snapshots {
            old: Some(original.clone()),
            new: None,
        }),
    }));
    assert!(restore_next(&stack).is_err());
    assert_eq!(stack.lock().unwrap().entries.len(), 1);
    restore_next(&stack).unwrap();
    same(&mode(kb.stdin_fd()), &original);
}
