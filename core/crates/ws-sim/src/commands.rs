//! Typed ingress and bounded canonical wire codec (ADR 0019).
use crate::{EntityId, IllnessKind, Lod, SimTime, TimeScale};
/// Maximum incoming/pending canonical command bytes.
pub const MAX_BYTES: usize = 65_536;
/// Maximum incoming/pending commands.
pub const MAX_COMMANDS: usize = 1024;
/// Who issued a command; never an authorization credential.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IssuedBy {
    /// Player input.
    Player = 0,
    /// Declared script.
    Script = 1,
    /// Test harness.
    Test = 2,
}
/// Version-one command payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Journaled no-op.
    NoOp,
    /// Authoritative pacing scale.
    SetTimeScale(TimeScale),
    /// Atomic building-level request.
    SetNodeLod(Vec<(EntityId, Lod)>),
    /// A structured god intervention (ADR 0021).
    Intervene(Intervention),
}

/// Version of the closed effect vocabulary (ADR 0021 §1).
pub const EFFECT_VOCABULARY_VERSION: u32 = 1;

/// Who an intervention affects. Resolved at apply time in canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterventionTarget {
    /// One resident.
    Entity(EntityId),
    /// Every resident of one building.
    ResidentsOf(EntityId),
}

/// An effect from the closed vocabulary. Never code supplied at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Makes the target ill; supports levels L1 and L2.
    Afflict {
        /// Kind of illness.
        illness: IllnessKind,
        /// Severity, `SEVERITY_MIN..=SEVERITY_MAX`.
        severity: u8,
    },
}

/// When an intervention takes effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    /// At the boundary where it is processed.
    Now,
    /// At the first tick boundary at or after `SimTime`; a time already
    /// reached applies now.
    At(SimTime),
}

/// `{ target, effect, timing }`; provenance is the journal entry itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Intervention {
    /// Who is affected.
    pub target: InterventionTarget,
    /// What happens.
    pub effect: Effect,
    /// When.
    pub timing: Timing,
}
/// Owned command envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// Schema, currently 1.
    pub schema_version: u32,
    /// Declared source.
    pub issued_by: IssuedBy,
    /// Typed payload.
    pub action: Action,
}
impl Command {
    /// Creates a version-one envelope.
    pub fn new(issued_by: IssuedBy, action: Action) -> Self {
        Self {
            schema_version: 1,
            issued_by,
            action,
        }
    }
    /// Canonical encoded bytes excluding the batch header.
    pub fn encoded_len(&self) -> usize {
        16 + match &self.action {
            Action::NoOp => 0,
            Action::SetTimeScale(_) => 4,
            Action::SetNodeLod(v) => 4 + 12 * v.len(),
            Action::Intervene(_) => INTERVENE_BYTES,
        }
    }
    /// Stable opcode.
    pub fn opcode(&self) -> u32 {
        match self.action {
            Action::NoOp => 0,
            Action::SetTimeScale(_) => 1,
            Action::SetNodeLod(_) => 2,
            Action::Intervene(_) => 3,
        }
    }
}
/// Wire payload bytes of an intervention: target kind, ID (2 words), effect,
/// illness, severity, timing kind, time (2 words).
const INTERVENE_BYTES: usize = 36;

/// Ingress rejects an entire batch without allocating sequences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum IngressError {
    /// Invalid framing, count, tag, or length.
    Malformed = 1,
    /// Unsupported envelope/schema.
    Version = 2,
    /// Queue or batch capacity exhausted.
    Capacity = 3,
    /// Buffer reservation failed.
    Resource = 4,
}
impl std::fmt::Display for IngressError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "command ingress: {self:?}")
    }
}
impl std::error::Error for IngressError {}
/// Successful atomic ingress metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Accepted {
    /// Number copied to the queue.
    pub count: u32,
    /// First assigned sequence, zero for empty batch.
    pub first_sequence: u64,
}
/// Rejection independent of transport or allocator failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// Typed envelope schema unsupported.
    Schema,
    /// More than 1024 LOD entries.
    TooManyNodes,
    /// Domain rejected the request without mutation.
    Lod(crate::LodError),
    /// An intervention names an entity that is not a resident / building.
    UnknownTarget(EntityId),
    /// A target resident is at a simulation level the effect does not support.
    UnsupportedLevel(EntityId),
    /// Severity outside the accepted range.
    SeverityOutOfRange(u8),
}
/// One committed outcome; infrastructure failures have no outcome yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Journal record schema, currently one.
    pub schema_version: u32,
    /// Assigned enqueue sequence.
    pub sequence: u64,
    /// Integer boundary time.
    pub time: crate::SimTime,
    /// Actual effect: scale tag or changed-node count.
    pub effect: u32,
    /// Demoted residents.
    pub demoted: u32,
    /// Promoted residents.
    pub promoted: u32,
    /// Current physical tick.
    pub tick: u64,
    /// Nonempty flush ordinal.
    pub boundary: u64,
    /// Owned original command.
    pub command: Command,
    /// Applied, or a deterministic semantic rejection.
    pub result: Result<(), Rejection>,
}
/// Encode explicit little-endian fields, never Rust memory layout.
pub fn encode_batch(commands: &[Command]) -> Result<Vec<u8>, IngressError> {
    let bytes = validate_batch(commands)?;
    let mut out = Vec::new();
    out.try_reserve(bytes).map_err(|_| IngressError::Resource)?;
    put(&mut out, 2);
    put(&mut out, commands.len() as u32);
    for c in commands {
        put(&mut out, c.opcode());
        put(&mut out, (c.encoded_len() - 8) as u32);
        put(&mut out, c.schema_version);
        put(&mut out, c.issued_by as u32);
        match &c.action {
            Action::NoOp => {}
            Action::SetTimeScale(s) => put(&mut out, s.multiplier()),
            Action::SetNodeLod(v) => {
                put(&mut out, v.len() as u32);
                for (id, l) in v {
                    out.extend_from_slice(&id.raw().to_le_bytes());
                    put(
                        &mut out,
                        match l {
                            Lod::L1 => 1,
                            Lod::L2 => 2,
                            _ => return Err(IngressError::Malformed),
                        },
                    );
                }
            }
            Action::Intervene(i) => {
                let (kind, id) = match i.target {
                    InterventionTarget::Entity(id) => (0, id),
                    InterventionTarget::ResidentsOf(id) => (1, id),
                };
                put(&mut out, kind);
                out.extend_from_slice(&id.raw().to_le_bytes());
                let Effect::Afflict { illness, severity } = i.effect;
                put(&mut out, 0);
                put(&mut out, u32::from(illness.tag()));
                put(&mut out, u32::from(severity));
                let (tk, micros) = match i.timing {
                    Timing::Now => (0, 0),
                    Timing::At(t) => (1, t.as_micros_since_epoch()),
                };
                put(&mut out, tk);
                out.extend_from_slice(&micros.to_le_bytes());
            }
        }
    }
    Ok(out)
}
pub(crate) fn validate_batch(commands: &[Command]) -> Result<usize, IngressError> {
    let bytes = commands
        .iter()
        .try_fold(8usize, |n, c| n.checked_add(c.encoded_len()))
        .ok_or(IngressError::Capacity)?;
    if commands.len() > MAX_COMMANDS || bytes > MAX_BYTES {
        return Err(IngressError::Capacity);
    }
    Ok(bytes)
}
fn put(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}
/// Decodes a bounded complete batch before enqueue; unknown nodes are semantic.
pub fn decode_batch(bytes: &[u8]) -> Result<Vec<Command>, IngressError> {
    validate_wire_batch(bytes)?;
    decode_owned(bytes, true)
}
/// Allocation-free structural preflight; allocator stops cannot hide malformed input.
pub fn validate_wire_batch(bytes: &[u8]) -> Result<usize, IngressError> {
    decode_owned(bytes, false)?;
    Ok(u32::from_le_bytes(bytes[4..8].try_into().expect("validated header")) as usize)
}
fn decode_owned(bytes: &[u8], owned: bool) -> Result<Vec<Command>, IngressError> {
    if bytes.len() > MAX_BYTES {
        return Err(IngressError::Capacity);
    }
    let mut r = Reader { bytes, at: 0 };
    if r.u32()? != 2 {
        return Err(IngressError::Version);
    }
    let count = r.u32()? as usize;
    if count > MAX_COMMANDS {
        return Err(IngressError::Capacity);
    }
    let mut out = Vec::new();
    if owned {
        out.try_reserve(count).map_err(|_| IngressError::Resource)?;
    }
    for _ in 0..count {
        let op = r.u32()?;
        let len = r.u32()? as usize;
        let end =
            r.at.checked_add(len)
                .filter(|n| *n <= bytes.len())
                .ok_or(IngressError::Malformed)?;
        let schema = r.u32()?;
        if schema != 1 {
            return Err(IngressError::Version);
        }
        let by = match r.u32()? {
            0 => IssuedBy::Player,
            1 => IssuedBy::Script,
            2 => IssuedBy::Test,
            _ => return Err(IngressError::Malformed),
        };
        let action = match op {
            0 => Action::NoOp,
            1 => Action::SetTimeScale(match r.u32()? {
                0 => TimeScale::Paused,
                1 => TimeScale::X1,
                10 => TimeScale::X10,
                100 => TimeScale::X100,
                _ => return Err(IngressError::Malformed),
            }),
            2 => {
                let n = r.u32()? as usize;
                if n > 1024 || len != 12 + 12 * n {
                    return Err(IngressError::Malformed);
                }
                let mut v = Vec::new();
                if owned {
                    v.try_reserve(n).map_err(|_| IngressError::Resource)?;
                }
                for _ in 0..n {
                    let lo = r.u32()? as u64;
                    let hi = r.u32()? as u64;
                    let id =
                        EntityId::from_raw(lo | (hi << 32)).map_err(|_| IngressError::Malformed)?;
                    let lod = match r.u32()? {
                        1 => Lod::L1,
                        2 => Lod::L2,
                        _ => return Err(IngressError::Malformed),
                    };
                    if owned {
                        v.push((id, lod));
                    }
                }
                Action::SetNodeLod(v)
            }
            3 => {
                if len != 8 + INTERVENE_BYTES {
                    return Err(IngressError::Malformed);
                }
                let kind = r.u32()?;
                let id = r.id()?;
                let target = match kind {
                    0 => InterventionTarget::Entity(id),
                    1 => InterventionTarget::ResidentsOf(id),
                    _ => return Err(IngressError::Malformed),
                };
                if r.u32()? != 0 {
                    return Err(IngressError::Malformed);
                }
                let illness = u8::try_from(r.u32()?)
                    .ok()
                    .and_then(IllnessKind::from_tag)
                    .ok_or(IngressError::Malformed)?;
                // Out-of-range severity is a semantic rejection, not framing.
                let severity = u8::try_from(r.u32()?).unwrap_or(u8::MAX);
                let timing = match r.u32()? {
                    0 if r.i64()? == 0 => Timing::Now,
                    1 => Timing::At(SimTime::from_micros_since_epoch(r.i64()?)),
                    _ => return Err(IngressError::Malformed),
                };
                Action::Intervene(Intervention {
                    target,
                    effect: Effect::Afflict { illness, severity },
                    timing,
                })
            }
            _ => return Err(IngressError::Malformed),
        };
        if r.at != end {
            return Err(IngressError::Malformed);
        }
        if owned {
            out.push(Command {
                schema_version: schema,
                issued_by: by,
                action,
            });
        }
    }
    if r.at != bytes.len() {
        return Err(IngressError::Malformed);
    }
    Ok(out)
}
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Reader<'_> {
    fn id(&mut self) -> Result<EntityId, IngressError> {
        let lo = u64::from(self.u32()?);
        let hi = u64::from(self.u32()?);
        EntityId::from_raw(lo | (hi << 32)).map_err(|_| IngressError::Malformed)
    }
    fn i64(&mut self) -> Result<i64, IngressError> {
        let lo = u64::from(self.u32()?);
        let hi = u64::from(self.u32()?);
        Ok((lo | (hi << 32)) as i64)
    }
    fn u32(&mut self) -> Result<u32, IngressError> {
        let s = self
            .bytes
            .get(self.at..self.at + 4)
            .ok_or(IngressError::Malformed)?;
        self.at += 4;
        Ok(u32::from_le_bytes(s.try_into().expect("four bytes")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_two_wire_fixture_is_explicit_and_round_trips() {
        let hex = concat!(
            "0200000003000000",
            "00000000080000000100000002000000",
            "010000000c00000001000000000000000a000000",
            "02000000180000000100000001000000010000002a0000000000008002000000"
        );
        let fixture: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let commands = vec![
            Command::new(IssuedBy::Test, Action::NoOp),
            Command::new(IssuedBy::Player, Action::SetTimeScale(TimeScale::X10)),
            Command::new(
                IssuedBy::Script,
                Action::SetNodeLod(vec![(EntityId::spawned(42).unwrap(), Lod::L2)]),
            ),
        ];
        assert_eq!(fixture.len(), 76);
        assert_eq!(validate_wire_batch(&fixture), Ok(3));
        assert_eq!(encode_batch(&commands).unwrap(), fixture);
        assert_eq!(decode_batch(&fixture).unwrap(), commands);
        for i in [0, 8, 12, 16, 20, 24, 28, 32, 36, 40, 44, 48, 52, 56, 60, 72] {
            let mut changed = fixture.clone();
            changed[i] = 255;
            assert!(decode_batch(&changed).is_err(), "invalid field at {i}");
        }
    }
}
