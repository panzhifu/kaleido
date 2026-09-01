//! The animation timeline — dual-track model.
//!
//! Two independent animation channels coexist in one document:
//!
//! 1. **Hand-drawn frame animation** (Krita style): each
//!    [`PixelLayer`](crate::pixel_layer::PixelLayer) carries per-frame pixel
//!    snapshots.  The timeline only tracks the frame rate and duration.
//! 2. **Property keyframes** (After Effects style): [`Track`]s bind a node
//!    property to a list of [`Keyframe`]s, driven by the playhead.

use serde::{Deserialize, Serialize};

use super::types::{Color, NodeId};

/// Default frame rate for new documents (fps).
pub const DEFAULT_FRAME_RATE: u32 = 24;

/// The animation timeline of a document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Timeline {
    pub frame_rate: u32,
    /// Total number of frames (playhead range is `0..duration`).
    pub duration: u32,
    /// Property-keyframe tracks (AE-style animation).
    pub tracks: Vec<Track>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self {
            frame_rate: DEFAULT_FRAME_RATE,
            duration: 1,
            tracks: Vec::new(),
        }
    }
}

impl Timeline {
    /// Creates a timeline with the given frame rate.
    #[inline]
    pub fn new(frame_rate: u32) -> Self {
        Self {
            frame_rate,
            duration: 1,
            tracks: Vec::new(),
        }
    }

    /// Appends an empty track bound to `node` / `prop` and returns it.
    pub fn add_track(&mut self, node: NodeId, prop: AnimatableProp) -> &mut Track {
        self.tracks.push(Track::new(node, prop));
        self.tracks.last_mut().expect("track was just pushed")
    }

    /// Finds the track binding `node` + `prop`.
    pub fn track(&self, node: NodeId, prop: AnimatableProp) -> Option<&Track> {
        self.tracks
            .iter()
            .find(|t| t.node == node && t.prop == prop)
    }

    /// Finds the track binding `node` + `prop`, mutably.
    pub fn track_mut(&mut self, node: NodeId, prop: AnimatableProp) -> Option<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|t| t.node == node && t.prop == prop)
    }

    /// Removes the track binding `node` + `prop`.  Returns whether one was removed.
    pub fn remove_track(&mut self, node: NodeId, prop: AnimatableProp) -> bool {
        let before = self.tracks.len();
        self.tracks
            .retain(|t| !(t.node == node && t.prop == prop));
        self.tracks.len() != before
    }

    /// Samples the value of `prop` on `node` at `frame` (if a track exists).
    #[inline]
    pub fn sample(&self, node: NodeId, prop: AnimatableProp, frame: u32) -> Option<AnimValue> {
        self.track(node, prop).and_then(|t| t.sample(frame))
    }

    /// Converts a frame index to seconds at the timeline's frame rate.
    #[inline]
    pub fn frame_to_seconds(&self, frame: u32) -> f32 {
        frame as f32 / self.frame_rate.max(1) as f32
    }

    /// Converts seconds to the nearest frame index.
    #[inline]
    pub fn seconds_to_frame(&self, seconds: f32) -> u32 {
        (seconds * self.frame_rate.max(1) as f32).round().max(0.0) as u32
    }
}

/// Which animatable property a track drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnimatableProp {
    /// Node transform (position / rotation / scale).
    Transform,
    /// Node opacity (0.0 – 1.0).
    Opacity,
    /// Vector fill color.
    FillColor,
    /// Stroke color.
    StrokeColor,
    /// Effect parameter (identified by effect binding index + param name).
    EffectParam,
}

/// A keyframe value — union of the supported animated value types.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AnimValue {
    /// 2D vector (position, scale…).
    Vec2(f32, f32),
    /// Scalar (opacity, rotation angle…).
    Scalar(f32),
    /// Color.
    Color(Color),
    /// Boolean (visibility toggles).
    Bool(bool),
}

impl AnimValue {
    /// Component-wise linear interpolation between two values of the same
    /// variant at progress `t` (0.0 – 1.0).
    ///
    /// If the variants differ, the earlier value is returned unchanged
    /// (callers should not mix variants on one track).
    pub fn lerp(self, other: Self, t: f32) -> Self {
        match (self, other) {
            (AnimValue::Vec2(x0, y0), AnimValue::Vec2(x1, y1)) => {
                AnimValue::Vec2(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t)
            }
            (AnimValue::Scalar(a), AnimValue::Scalar(b)) => AnimValue::Scalar(a + (b - a) * t),
            (AnimValue::Color(c0), AnimValue::Color(c1)) => AnimValue::Color(c0.lerp(c1, t)),
            (AnimValue::Bool(b0), AnimValue::Bool(b1)) => {
                AnimValue::Bool(if t < 0.5 { b0 } else { b1 })
            }
            (a, _) => a,
        }
    }
}

/// Interpolation between keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    /// Hold: value stays constant until the next keyframe.
    Hold,
}

impl Easing {
    /// Maps a linear progress `t` (0.0 – 1.0) through the easing curve,
    /// returning the eased progress.
    pub fn apply(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::EaseIn => t * t,
            Easing::EaseOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::EaseInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - 2.0 * (1.0 - t) * (1.0 - t)
                }
            }
            // Hold is handled by the sampler (value stays at the previous
            // keyframe); applying it to a progress returns 0 by definition.
            Easing::Hold => 0.0,
        }
    }
}

/// A single keyframe on a track.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Keyframe {
    /// Frame index within the timeline.
    pub frame: u32,
    pub value: AnimValue,
    pub easing: Easing,
}

impl Keyframe {
    #[inline]
    pub const fn new(frame: u32, value: AnimValue, easing: Easing) -> Self {
        Self {
            frame,
            value,
            easing,
        }
    }
}

/// A property-keyframe track bound to a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub node: NodeId,
    pub prop: AnimatableProp,
    /// For `EffectParam`, the index into the node's `effects` vec.
    pub effect_index: Option<usize>,
    /// Keyframes sorted by `frame`.
    pub keyframes: Vec<Keyframe>,
}

impl Track {
    #[inline]
    pub fn new(node: NodeId, prop: AnimatableProp) -> Self {
        Self {
            node,
            prop,
            effect_index: None,
            keyframes: Vec::new(),
        }
    }

    /// Creates a track driving an effect parameter.
    #[inline]
    pub fn for_effect(node: NodeId, effect_index: usize) -> Self {
        Self {
            node,
            prop: AnimatableProp::EffectParam,
            effect_index: Some(effect_index),
            keyframes: Vec::new(),
        }
    }

    /// Inserts a keyframe, keeping the list sorted by `frame`.  A keyframe
    /// at an existing frame replaces it.
    pub fn add_keyframe(&mut self, keyframe: Keyframe) {
        let idx = self.keyframes.partition_point(|k| k.frame < keyframe.frame);
        if self.keyframes.get(idx).is_some_and(|k| k.frame == keyframe.frame) {
            self.keyframes[idx] = keyframe;
        } else {
            self.keyframes.insert(idx, keyframe);
        }
    }

    /// Removes the keyframe at exactly `frame`.  Returns whether one was removed.
    pub fn remove_keyframe_at(&mut self, frame: u32) -> bool {
        let idx = self.keyframes.partition_point(|k| k.frame < frame);
        if self.keyframes.get(idx).is_some_and(|k| k.frame == frame) {
            self.keyframes.remove(idx);
            true
        } else {
            false
        }
    }

    /// Whether the track has any keyframes.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.keyframes.is_empty()
    }

    /// Samples the track's value at `frame`:
    ///
    /// - no keyframes → `None`;
    /// - exact hit → that keyframe's value;
    /// - before the first / after the last → the nearest keyframe's value;
    /// - between two keyframes → interpolation with the *outgoing* easing
    ///   (the previous keyframe's `easing`); `Hold` keeps the previous value.
    pub fn sample(&self, frame: u32) -> Option<AnimValue> {
        let kfs = &self.keyframes;
        if kfs.is_empty() {
            return None;
        }

        // Exact hit or before the first keyframe.
        let idx = kfs.partition_point(|k| k.frame <= frame);
        if idx == 0 {
            return Some(kfs[0].value); // frame ≤ first keyframe
        }
        if idx == kfs.len() {
            return Some(kfs[idx - 1].value); // frame ≥ last keyframe
        }

        let k0 = kfs[idx - 1];
        let k1 = kfs[idx];
        if k0.easing == Easing::Hold {
            return Some(k0.value);
        }
        let span = (k1.frame - k0.frame).max(1) as f32;
        let t = (frame - k0.frame) as f32 / span;
        Some(k0.value.lerp(k1.value, k0.easing.apply(t)))
    }
}
