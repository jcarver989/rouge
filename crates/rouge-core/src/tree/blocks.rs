use super::layers::LayerRef;
use super::{ElementId, Tree};
use crate::compositor::ListId;
use rouge_render::{Point, Rect};
use rustc_hash::FxHashMap;
use std::mem::{replace, take};

pub(super) const LOOSE: u32 = u32::MAX;

const FEWEST_CHILDREN: usize = 64;
const BLOCK: usize = 32;
const SMALLEST: usize = 16;
const LARGEST: usize = 48;

pub(super) type BlockRef = (ElementId, u32);

#[derive(Clone, Copy)]
pub(super) enum Piece {
    List { id: ListId, offset: Point },
    Nested { layer: LayerRef, clip: Rect },
}

impl Piece {
    pub(super) fn shifted(self, by: Point) -> Piece {
        match self {
            Piece::List { id, offset } => Piece::List {
                id,
                offset: offset + by,
            },
            Piece::Nested { layer, clip } => Piece::Nested {
                layer,
                clip: shifted(clip, by),
            },
        }
    }
}

pub(super) fn shifted(clip: Rect, by: Point) -> Rect {
    if clip == Rect::EVERYTHING {
        clip
    } else {
        clip.translate(by)
    }
}

pub(super) struct Block {
    pub(super) id: u64,
    pub(super) children: Vec<ElementId>,
    pub(super) offset: Point,
    pub(super) dirty: bool,
    pub(super) phase: Point,
    pub(super) clip: Rect,
    pub(super) pieces: Vec<Piece>,
}

impl Block {
    fn new(id: u64, children: Vec<ElementId>) -> Self {
        Block {
            id,
            children,
            offset: Point::ZERO,
            dirty: true,
            phase: Point::ZERO,
            clip: Rect::EVERYTHING,
            pieces: Vec::new(),
        }
    }

    pub(super) fn lead(&self) -> Option<ElementId> {
        self.children.first().copied()
    }
}

#[derive(Default)]
pub(super) struct Blocks {
    pub(super) blocks: Vec<Block>,
    pub(super) stale: bool,
}

impl Blocks {
    fn regroup(&mut self, children: &[ElementId], next: &mut u64) {
        let mut known = take(&mut self.blocks);
        let leads: FxHashMap<ElementId, usize> = known
            .iter()
            .enumerate()
            .filter_map(|(index, block)| Some((block.lead()?, index)))
            .collect();
        let mut run = Vec::new();
        let mut rest = children;
        while let [child, after @ ..] = rest {
            let block = leads.get(child).map(|index| &mut known[*index]);
            let whole = block.filter(|block| {
                let count = block.children.len();
                let joins = !run.is_empty() && run.len() < SMALLEST && run.len() + count <= LARGEST;
                count >= SMALLEST && !joins && rest.starts_with(&block.children)
            });
            if let Some(block) = whole {
                self.add(take(&mut run), next);
                rest = &rest[block.children.len()..];
                let kept = replace(block, Block::new(0, Vec::new()));
                self.blocks.push(kept);
            } else {
                run.push(*child);
                rest = after;
            }
        }
        self.add(run, next);
        self.stale = false;
    }

    fn add(&mut self, run: Vec<ElementId>, next: &mut u64) {
        let parts = run.len().div_ceil(BLOCK);
        for part in 0..parts {
            let (start, end) = (part * run.len() / parts, (part + 1) * run.len() / parts);
            *next += 1;
            let children = run[start..end].to_vec();
            self.blocks.push(Block::new(*next, children));
        }
    }
}

impl Tree {
    pub(super) fn sync_blocks(&mut self, id: ElementId) {
        let element = &mut self.elements[id];
        if element.children.len() < FEWEST_CHILDREN {
            if element.blocks.take().is_some() {
                for index in 0..self.elements[id].children.len() {
                    let child = self.elements[id].children[index];
                    self.elements[child].block = LOOSE;
                }
            }
            return;
        }
        if element.blocks.as_ref().is_some_and(|blocks| !blocks.stale) {
            return;
        }
        let mut blocks = element.blocks.take().unwrap_or_default();
        blocks.regroup(&element.children, &mut self.next_block);
        for (index, block) in blocks.blocks.iter().enumerate() {
            for child in &block.children {
                self.elements[*child].block = index as u32;
            }
        }
        self.elements[id].blocks = Some(blocks);
    }

    pub(super) fn mark_around(&mut self, id: ElementId) {
        let element = &self.elements[id];
        if let Some(parent) = element.parent {
            self.mark_block((parent, element.block));
        }
    }

    pub(super) fn mark_block(&mut self, (host, index): BlockRef) {
        if let Some(block) = self.elements[host].block_mut(index) {
            block.dirty = true;
        }
        self.mark_layer(host, true);
    }
}
