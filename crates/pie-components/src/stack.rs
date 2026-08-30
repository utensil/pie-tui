//! Stack layout sizing — entries with basis/grow/shrink/min/max constraints
//! and the reference's proportional distribute algorithm
//! (reference `components/stack.js`).

/// Per-child layout constraints for stack containers.
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StackViewport {
    pub width: usize,
    pub height: usize,
}

pub type StackVisibilityFn = Arc<dyn Fn(StackViewport) -> bool + Send + Sync>;

#[derive(Clone)]
pub struct StackEntry {
    /// Preferred size; `None` = "auto" (use intrinsic size).
    pub basis: Option<usize>,
    /// Flex-grow weight (default 0).
    pub grow: usize,
    /// Flex-shrink weight (default 1).
    pub shrink: usize,
    pub min_size: usize,
    pub max_size: usize,
    pub visible: Option<StackVisibilityFn>,
}

impl StackEntry {
    pub fn auto() -> Self {
        StackEntry {
            basis: None,
            grow: 0,
            shrink: 1,
            min_size: 0,
            max_size: usize::MAX,
            visible: None,
        }
    }
}

impl Default for StackEntry {
    fn default() -> Self {
        Self::auto()
    }
}

impl StackEntry {
    pub fn is_visible(&self, viewport: StackViewport) -> bool {
        self.visible
            .as_ref()
            .is_none_or(|predicate| predicate(viewport))
    }
}

fn clamp_size(size: usize, entry: &StackEntry) -> usize {
    let min = entry.min_size;
    let max = entry.max_size.max(min);
    size.clamp(min, max)
}

/// Distribute `amount` among `sizes` proportionally (reference `distribute`).
/// Grow weights by `entry.grow`; shrink weights by `entry.shrink * max(1, size)`.
fn distribute(sizes: &mut [usize], entries: &[StackEntry], amount: usize, mode: Mode) {
    let mut remaining = amount;
    while remaining > 0 {
        struct Cand {
            index: usize,
            weight: f64,
        }
        let candidates: Vec<Cand> = entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| match mode {
                Mode::Grow => entry.grow > 0 && sizes[*index] < entry.max_size,
                Mode::Shrink => entry.shrink > 0 && sizes[*index] > entry.min_size,
            })
            .map(|(index, entry)| Cand {
                index,
                weight: match mode {
                    Mode::Grow => entry.grow as f64,
                    Mode::Shrink => entry.shrink as f64 * sizes[index].max(1) as f64,
                },
            })
            .collect();
        if candidates.is_empty() {
            return;
        }
        // The reference performs proportional arithmetic as JavaScript
        // `Number`. Keeping the weights and ratio in f64 both avoids machine
        // integer overflow and matches its MAX_SAFE_INTEGER rounding.
        let total_weight: f64 = candidates.iter().map(|candidate| candidate.weight).sum();
        let mut distributed = 0usize;
        for cand in &candidates {
            if remaining == 0 {
                break;
            }
            let entry = &entries[cand.index];
            let proposed = ((remaining as f64 * cand.weight) / total_weight)
                .floor()
                .max(1.0) as usize;
            let capacity = match mode {
                Mode::Grow => entry.max_size.saturating_sub(sizes[cand.index]),
                Mode::Shrink => sizes[cand.index].saturating_sub(entry.min_size),
            };
            let delta = remaining.min(proposed).min(capacity);
            if delta == 0 {
                continue;
            }
            match mode {
                Mode::Grow => sizes[cand.index] += delta,
                Mode::Shrink => sizes[cand.index] -= delta,
            }
            remaining -= delta;
            distributed += delta;
        }
        if distributed == 0 {
            return;
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Grow,
    Shrink,
}

/// Allocate final sizes for stack entries given intrinsic sizes and the
/// available size (reference `allocateStackSizes`).
pub fn allocate_stack_sizes(
    entries: &[StackEntry],
    intrinsic_sizes: &[usize],
    available_size: Option<usize>,
    gap: usize,
) -> Vec<usize> {
    let sizes: Vec<usize> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            // basis None = "auto": fall back to the intrinsic size.
            let base = match entry.basis {
                Some(basis) => basis,
                None => intrinsic_sizes.get(index).copied().unwrap_or(0),
            };
            clamp_size(base, entry)
        })
        .collect();
    let Some(available_size) = available_size else {
        return sizes;
    };
    let content_size = available_size.saturating_sub(entries.len().saturating_sub(1) * gap);
    let total: usize = sizes.iter().sum();
    if total < content_size {
        let mut sizes = sizes;
        distribute(&mut sizes, entries, content_size - total, Mode::Grow);
        sizes
    } else if total > content_size {
        let mut sizes = sizes;
        distribute(&mut sizes, entries, total - content_size, Mode::Shrink);
        sizes
    } else {
        sizes
    }
}
