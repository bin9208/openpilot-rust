//! NumPy2.5.3 portable Float32 argsort, including unstable equal-key order.
//! Source sdist df2d5874ff183595a4ba404edd04f6bd9b5505c1d7708573f6a6c17489a67563,
//! npysort/quicksort.hpp + npysort_heapsort.h; see NUMPY_LICENSE.
//! ARM has no x86 argsort dispatch and executes this portable path.
fn less(a: f32, b: f32) -> bool {
    a < b || b.is_nan() && !a.is_nan()
}
fn heap(values: &[f32], indices: &mut [usize]) {
    let mut n = indices.len();
    for start in (1..=n / 2).rev() {
        let item = indices[start - 1];
        let mut i = start;
        let mut j = start * 2;
        while j <= n {
            if j < n && less(values[indices[j - 1]], values[indices[j]]) {
                j += 1;
            }
            if !less(values[item], values[indices[j - 1]]) {
                break;
            }
            indices[i - 1] = indices[j - 1];
            i = j;
            j *= 2;
        }
        indices[i - 1] = item;
    }
    while n > 1 {
        let item = indices[n - 1];
        indices[n - 1] = indices[0];
        n -= 1;
        let mut i = 1;
        let mut j = 2;
        while j <= n {
            if j < n && less(values[indices[j - 1]], values[indices[j]]) {
                j += 1;
            }
            if !less(values[item], values[indices[j - 1]]) {
                break;
            }
            indices[i - 1] = indices[j - 1];
            i = j;
            j *= 2;
        }
        indices[i - 1] = item;
    }
}
pub fn scalar_argsort(values: &[f32]) -> Vec<usize> {
    let mut indices = (0..values.len()).collect::<Vec<_>>();
    if indices.len() < 2 {
        return indices;
    }
    let mut left = 0;
    let mut right = indices.len() - 1;
    let mut depth = (usize::BITS - 1 - indices.len().leading_zeros()) as i32 * 2;
    let mut stack = Vec::new();
    loop {
        if depth < 0 {
            heap(values, &mut indices[left..=right]);
        } else {
            while right - left > 15 {
                let middle = left + ((right - left) >> 1);
                if less(values[indices[middle]], values[indices[left]]) {
                    indices.swap(middle, left);
                }
                if less(values[indices[right]], values[indices[middle]]) {
                    indices.swap(right, middle);
                }
                if less(values[indices[middle]], values[indices[left]]) {
                    indices.swap(middle, left);
                }
                let pivot = values[indices[middle]];
                let mut i = left;
                let mut j = right - 1;
                indices.swap(middle, j);
                loop {
                    loop {
                        i += 1;
                        if !less(values[indices[i]], pivot) {
                            break;
                        }
                    }
                    loop {
                        j -= 1;
                        if !less(pivot, values[indices[j]]) {
                            break;
                        }
                    }
                    if i >= j {
                        break;
                    }
                    indices.swap(i, j);
                }
                indices.swap(i, right - 1);
                depth -= 1;
                if i - left < right - i {
                    stack.push((i + 1, right, depth));
                    right = i - 1;
                } else {
                    stack.push((left, i - 1, depth));
                    left = i + 1;
                }
            }
            for i in left + 1..=right {
                let item = indices[i];
                let pivot = values[item];
                let mut j = i;
                while j > left && less(pivot, values[indices[j - 1]]) {
                    indices[j] = indices[j - 1];
                    j -= 1;
                }
                indices[j] = item;
            }
        }
        let Some(next) = stack.pop() else {
            break;
        };
        (left, right, depth) = next;
    }
    indices
}

/// Source architecture dispatch. The ARM source always uses the scalar owner.
pub fn argsort(values: &[f32]) -> Vec<usize> {
    #[cfg(feature = "neural-numerics")]
    if let Some(order) = openpilot_signal_numerics::argsort(values) {
        return order;
    }
    scalar_argsort(values)
}
