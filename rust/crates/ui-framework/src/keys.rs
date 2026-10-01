use std::{
    cell::RefCell,
    collections::{BTreeSet, VecDeque},
};
pub const RIGHT: i32 = 262;
pub const LEFT: i32 = 263;
pub const BACKSPACE: i32 = 259;
pub const DELETE: i32 = 261;
pub const HOME: i32 = 268;
pub const END: i32 = 269;
pub const ENTER: i32 = 257;
pub const ESCAPE: i32 = 256;
#[derive(Default)]
pub struct KeyboardInput {
    pub queued: RefCell<VecDeque<i32>>,
    pub characters: RefCell<VecDeque<u32>>,
    pub down: BTreeSet<i32>,
    pub pressed: BTreeSet<i32>,
}
impl KeyboardInput {
    pub fn key(&self) -> i32 {
        self.queued.borrow_mut().pop_front().unwrap_or(0)
    }
    pub fn character(&self) -> Option<char> {
        self.characters
            .borrow_mut()
            .pop_front()
            .and_then(char::from_u32)
    }
}
