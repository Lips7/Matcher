use std::cell::UnsafeCell;

use tinyvec::TinyVec;

use super::rule::RuleHot;

#[derive(Default, Clone, Copy)]
pub(super) struct WordState {
    pub(super) matrix_generation: u32,
    pub(super) positive_generation: u32,
    pub(super) not_generation: u32,
    pub(super) satisfied_mask: u64,
    pub(super) remaining_and: u16,
}

pub(super) struct SimpleMatchState {
    pub(super) word_states: Vec<WordState>,
    pub(super) matrix: Vec<TinyVec<[i32; 16]>>,
    pub(super) matrix_status: Vec<TinyVec<[u8; 16]>>,
    pub(super) touched_indices: Vec<usize>,
    generation: u32,
}

#[thread_local]
pub(super) static SIMPLE_MATCH_STATE: UnsafeCell<SimpleMatchState> =
    UnsafeCell::new(SimpleMatchState::new());

#[derive(Clone, Copy)]
pub(super) struct ScanContext {
    pub(super) text_index: usize,
    pub(super) process_type_mask: u64,
    pub(super) num_variants: usize,
    pub(super) exit_early: bool,
    pub(super) is_ascii: bool,
}

impl SimpleMatchState {
    pub(super) const fn new() -> Self {
        Self {
            word_states: Vec::new(),
            matrix: Vec::new(),
            matrix_status: Vec::new(),
            touched_indices: Vec::new(),
            generation: 0,
        }
    }

    pub(super) fn prepare(&mut self, size: usize) {
        if self.generation == u32::MAX {
            for state in self.word_states.iter_mut() {
                state.matrix_generation = 0;
                state.positive_generation = 0;
                state.not_generation = 0;
            }
            self.generation = 1;
        } else {
            self.generation += 1;
        }

        if self.word_states.len() < size {
            self.word_states.resize(size, WordState::default());
            self.matrix.resize(size, TinyVec::new());
            self.matrix_status.resize(size, TinyVec::new());
        }

        self.touched_indices.clear();
    }

    #[inline(always)]
    pub(super) fn generation(&self) -> u32 {
        self.generation
    }

    #[inline(always)]
    pub(super) fn touched_indices(&self) -> &[usize] {
        &self.touched_indices
    }

    #[inline(always)]
    pub(super) fn rule_is_satisfied(&self, rule_idx: usize) -> bool {
        debug_assert!(rule_idx < self.word_states.len());
        let word_state = unsafe { self.word_states.get_unchecked(rule_idx) };
        word_state.positive_generation == self.generation
            && word_state.not_generation != self.generation
    }

    #[inline(always)]
    pub(super) fn mark_positive(&mut self, rule_idx: usize) -> bool {
        let generation = self.generation;
        debug_assert!(rule_idx < self.word_states.len());
        let word_state = unsafe { self.word_states.get_unchecked_mut(rule_idx) };
        if word_state.positive_generation == generation {
            return false;
        }
        if word_state.matrix_generation != generation {
            word_state.matrix_generation = generation;
            self.touched_indices.push(rule_idx);
        }
        word_state.positive_generation = generation;
        true
    }

    #[inline(always)]
    pub(super) fn init_rule(&mut self, rule: &RuleHot, rule_idx: usize, ctx: ScanContext) {
        let generation = self.generation;
        let word_state = unsafe { self.word_states.get_unchecked_mut(rule_idx) };
        word_state.matrix_generation = generation;
        word_state.positive_generation = if rule.and_count == 0 { generation } else { 0 };
        word_state.remaining_and = rule.and_count as u16;
        word_state.satisfied_mask = 0;
        self.touched_indices.push(rule_idx);

        if rule.use_matrix {
            init_matrix(
                unsafe { self.matrix.get_unchecked_mut(rule_idx) },
                unsafe { self.matrix_status.get_unchecked_mut(rule_idx) },
                &rule.segment_counts,
                ctx.num_variants,
            );
        }
    }
}

#[cold]
#[inline(never)]
fn init_matrix(
    flat_matrix: &mut TinyVec<[i32; 16]>,
    flat_status: &mut TinyVec<[u8; 16]>,
    segment_counts: &[i32],
    num_variants: usize,
) {
    let num_splits = segment_counts.len();
    flat_matrix.clear();
    flat_matrix.resize(num_splits * num_variants, 0i32);
    flat_status.clear();
    flat_status.resize(num_splits, 0u8);

    for (split_idx, &count) in segment_counts.iter().enumerate() {
        let row_start = split_idx * num_variants;
        flat_matrix[row_start..row_start + num_variants].fill(count);
    }
}
