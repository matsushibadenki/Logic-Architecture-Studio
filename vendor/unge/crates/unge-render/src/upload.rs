use std::ops::Range;

/// Instance-aligned writes, coalesced to bound queue overhead. The target keeps
/// its previous contents; newly allocated buffers must request a full upload.
pub fn changed_ranges(old: &[u8], new: &[u8], stride: usize) -> Vec<Range<usize>> {
    assert!(stride > 0 && new.len().is_multiple_of(stride));
    let mut ranges: Vec<Range<usize>> = Vec::new();
    // Retained frames often have long runs of unchanged instances.
    // Compare those runs in bulk before looking at individual instances.
    let block_size = stride.saturating_mul(32);
    let merge_gap = stride.saturating_mul(4);
    for (block_index, block) in new.chunks(block_size).enumerate() {
        let base = block_index * block_size;
        if old.get(base..base + block.len()) == Some(block) {
            continue;
        }
        for (index, value) in block.chunks_exact(stride).enumerate() {
            let start = base + index * stride;
            let end = start + stride;
            if old.get(start..end) == Some(value) {
                continue;
            }
            if let Some(last) = ranges
                .last_mut()
                .filter(|range| start <= range.end.saturating_add(merge_gap))
            {
                last.end = end;
            } else {
                if ranges.len() == 64 {
                    // Later edits can only extend the last range or append a
                    // new one, so there is no need to scan the rest.
                    return std::iter::once(0..new.len()).collect();
                }
                ranges.push(start..end);
            }
        }
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

    #[test]
    fn coalesces_across_blocks_and_stops_only_after_the_write_limit() {
        let old = vec![0u8; 512 * 4];
        let mut next = old.clone();
        next[31 * 4] = 1;
        next[36 * 4] = 1;
        assert_eq!(changed_ranges(&old, &next, 4), vec![31 * 4..37 * 4]);

        next.fill(0);
        for index in 0..64 {
            next[index * 6 * 4] = 1;
        }
        assert_eq!(changed_ranges(&old, &next, 4).len(), 64);
        next[64 * 6 * 4] = 1;
        assert_eq!(changed_ranges(&old, &next, 4), vec![0..next.len()]);
    }
}
