//! Hot-path scan and rule evaluation for [`super::SimpleMatcher`].

use crate::process::{ProcessedTextMasks, return_processed_string_to_pool, walk_process_tree};

use super::rule::PatternDispatch;
use super::state::{SIMPLE_MATCH_STATE, ScanContext, SimpleMatchState};
use super::{SimpleMatcher, SimpleResult};

impl SimpleMatcher {
    pub(super) fn is_match_simple(&self, text: &str) -> bool {
        self.scan.is_match(text)
    }

    #[inline(always)]
    pub(super) fn is_match_inner<const SINGLE_PT: bool>(&self, text: &str) -> bool {
        let tree = self.process.tree();
        let max_pt = tree.len();
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());
        let (text_masks, stopped) =
            walk_process_tree::<true, _>(tree, text, &mut |txt, idx, mask, is_ascii| {
                let ctx = ScanContext {
                    text_index: idx,
                    process_type_mask: mask,
                    num_variants: max_pt,
                    exit_early: true,
                    is_ascii,
                };
                self.scan_variant::<SINGLE_PT>(txt, ctx, state)
            });
        if stopped {
            return_processed_string_to_pool(text_masks);
            return true;
        }
        let result = self.rules.has_match(state);
        return_processed_string_to_pool(text_masks);
        result
    }

    pub(super) fn process_simple<'a>(&'a self, text: &'a str, results: &mut Vec<SimpleResult<'a>>) {
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());

        let _ = self
            .scan
            .for_each_match_value(text, text.is_ascii(), |raw_value| {
                match self.scan.patterns().dispatch::<true>(raw_value) {
                    PatternDispatch::DirectRule(rule_idx) => {
                        self.rules.push_result_if_new(rule_idx, state, results);
                    }
                    PatternDispatch::SingleEntry(entry) => {
                        self.rules
                            .push_result_if_new(entry.rule_idx as usize, state, results);
                    }
                    PatternDispatch::Entries(entries) => {
                        for entry in entries {
                            self.rules
                                .push_result_if_new(entry.rule_idx as usize, state, results);
                        }
                    }
                }
                false
            });
    }

    pub(super) fn process_preprocessed_into<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        results: &mut Vec<SimpleResult<'a>>,
    ) {
        let state = unsafe { &mut *SIMPLE_MATCH_STATE.get() };
        state.prepare(self.rules.len());

        self.scan_all_variants(processed_text_process_type_masks, state, false);
        self.rules.collect_matches(state, results);
    }

    pub(super) fn scan_all_variants<'a>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        if self.process.mode().single_pt_index().is_some() {
            self.scan_all_variants_inner::<true>(
                processed_text_process_type_masks,
                state,
                exit_early,
            )
        } else {
            self.scan_all_variants_inner::<false>(
                processed_text_process_type_masks,
                state,
                exit_early,
            )
        }
    }

    #[inline(always)]
    fn scan_all_variants_inner<'a, const SINGLE_PT: bool>(
        &'a self,
        processed_text_process_type_masks: &ProcessedTextMasks<'a>,
        state: &mut SimpleMatchState,
        exit_early: bool,
    ) -> bool {
        if self.scan.patterns().is_empty() {
            return false;
        }

        let num_variants = processed_text_process_type_masks.len();

        for (index, text_variant) in processed_text_process_type_masks.iter().enumerate() {
            if text_variant.mask == 0 {
                continue;
            }
            let ctx = ScanContext {
                text_index: index,
                process_type_mask: text_variant.mask,
                num_variants,
                exit_early,
                is_ascii: text_variant.is_ascii,
            };
            if self.scan_variant::<SINGLE_PT>(text_variant.text.as_ref(), ctx, state) {
                return true;
            }
        }

        false
    }

    #[inline(always)]
    pub(super) fn scan_variant<const SINGLE_PT: bool>(
        &self,
        processed_text: &str,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        self.scan
            .for_each_match_value(processed_text, ctx.is_ascii, |raw_value| {
                self.process_match::<SINGLE_PT>(raw_value, ctx, state)
            })
    }

    #[inline(always)]
    pub(super) fn process_match<const SINGLE_PT: bool>(
        &self,
        raw_value: u32,
        ctx: ScanContext,
        state: &mut SimpleMatchState,
    ) -> bool {
        match self.scan.patterns().dispatch::<SINGLE_PT>(raw_value) {
            PatternDispatch::DirectRule(rule_idx) => {
                state.mark_positive(rule_idx);
                ctx.exit_early
            }
            PatternDispatch::SingleEntry(entry) => {
                self.rules.process_entry::<SINGLE_PT>(entry, ctx, state)
            }
            PatternDispatch::Entries(entries) => {
                for entry in entries {
                    if self.rules.process_entry::<SINGLE_PT>(entry, ctx, state) {
                        return true;
                    }
                }
                false
            }
        }
    }
}
