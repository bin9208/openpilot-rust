use crate::policy::UploadItem;

#[derive(Default)]
pub struct Queue(pub Vec<UploadItem>);
impl Queue {
    pub fn push(&mut self, item: UploadItem) {
        self.0.push(item);
        self.sift_down(0, self.0.len() - 1);
    }

    pub fn pop(&mut self) -> Option<UploadItem> {
        let last = self.0.pop()?;
        if self.0.is_empty() {
            return Some(last);
        }
        let result = std::mem::replace(&mut self.0[0], last);
        let mut position = 0;
        let mut child = 1;
        while child < self.0.len() {
            let right = child + 1;
            if right < self.0.len() && self.0[child].priority >= self.0[right].priority {
                child = right;
            }
            self.0.swap(position, child);
            position = child;
            child = 2 * position + 1;
        }
        self.sift_down(0, position);
        Some(result)
    }

    fn sift_down(&mut self, start: usize, mut position: usize) {
        while position > start {
            let parent = (position - 1) >> 1;
            if self.0[position].priority >= self.0[parent].priority {
                break;
            }
            self.0.swap(position, parent);
            position = parent;
        }
    }
}
