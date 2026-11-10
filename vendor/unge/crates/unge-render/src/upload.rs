use std::ops::Range;

/// Instance-aligned writes, coalesced to bound queue overhead. The target keeps
/// its previous contents; newly allocated buffers must request a full upload.
pub fn changed_ranges(old: &[u8], new: &[u8], stride: usize) -> Vec<Range<usize>> {
    assert!(stride > 0 && new.len().is_multiple_of(stride));
    let mut ranges: Vec<Range<usize>> = Vec::new();
    for (i, value) in new.chunks_exact(stride).enumerate() {
        let start = i * stride;
        if old.get(start..start + stride) == Some(value) {
            continue;
        }
        if let Some(last) = ranges.last_mut().filter(|r| start <= r.end + stride * 4) {
            last.end = start + stride;
        } else {
            ranges.push(start..start + stride);
        }
    }
    if ranges.len() > 64 {
        return std::iter::once(0..new.len()).collect();
    }
    ranges
}

#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct UploadStats {
    pub quad_bytes: usize,
    pub quad_writes: usize,
    pub quad_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writes_reconstruct_edits_growth_shrink_and_fragmentation() {
        let old = vec![1u8; 4000];
        for len in [2000, 4000, 5000] {
            let mut next = vec![1u8; len];
            for i in (0..len).step_by(52) {
                next[i] = 7;
            }
            let mut gpu = old.clone();
            gpu.resize(len, 0);
            for r in changed_ranges(&old, &next, 4) {
                gpu[r.clone()].copy_from_slice(&next[r]);
            }
            assert_eq!(gpu, next);
        }
        assert!(changed_ranges(&old, &old, 4).is_empty());
    }
}
