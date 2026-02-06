use std::collections::vec_deque;
use std::collections::VecDeque;

/// A bounded buffer that replaces older values with newer ones when full
#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    buffer: VecDeque<T>,
    size: usize,
}

impl<T> Default for RingBuffer<T> {
    fn default() -> Self {
        RingBuffer {
            buffer: VecDeque::new(),
            size: 128,
        }
    }
}

impl<T> RingBuffer<T> {
    pub fn new(size: usize) -> Self {
        let buffer = VecDeque::new();
        RingBuffer { buffer, size }
    }

    pub fn push(&mut self, item: T) {
        if self.buffer.len() == self.size {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
    }

    pub fn take(&mut self) -> Self {
        let capacity = self.buffer.capacity();
        std::mem::replace(self, RingBuffer::new(capacity))
    }
}

impl<T> From<RingBuffer<T>> for Vec<T> {
    fn from(value: RingBuffer<T>) -> Self {
        value.buffer.into()
    }
}

impl<T> IntoIterator for RingBuffer<T> {
    type Item = T;
    type IntoIter = vec_deque::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.buffer.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_iter() {
        let mut ring_buffer = RingBuffer::new(3);

        ring_buffer.push(1);
        ring_buffer.push(2);
        ring_buffer.push(3);

        let result: Vec<_> = ring_buffer.into_iter().collect();
        assert_eq!(result, vec![1, 2, 3]);
    }

    #[test]
    fn push_wraps_around_when_full() {
        let mut ring_buffer = RingBuffer::new(3);

        ring_buffer.push(1);
        ring_buffer.push(2);
        ring_buffer.push(3);
        ring_buffer.push(4);

        let result: Vec<_> = ring_buffer.into_iter().collect();
        assert_eq!(result, vec![2, 3, 4]);
    }
}
