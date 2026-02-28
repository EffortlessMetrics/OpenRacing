//! Codemasters-style custom UDP packet decoding and XML specification support.
//! Supports the Dirt 4 / DiRT 4-legacy custom UDP format where each field is 4 bytes.
use anyhow::{Context, Result, anyhow};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

const FIELD_SIZE_BYTES: usize = 4;

#[derive(Debug, Clone)]
pub struct FieldSpec {
    pub channel: String,
    pub field_type: FieldType,
    pub scale: f32,
    pub fourcc_text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CustomUdpSpec {
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone)]
pub struct DecodedCodemastersPacket {
    pub values: HashMap<String, f32>,
    pub fourcc: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub enum FieldType {
    U32,
    I32,
    F32,
    FourCC,
}

#[derive(Default)]
struct PendingField {
    channel: Option<String>,
    field_type: Option<FieldType>,
    scale: Option<f32>,
    fourcc_text: Option<String>,
    pending_name: Option<String>,
}

impl FieldType {
    fn parse(raw: &str) -> Option<Self> {
        let normalized = canonical_channel_id(raw);
        if normalized.is_empty() {
            return None;
        }

        match normalized.as_str() {
            "uint32" | "u32" => Some(Self::U32),
            "int32" | "i32" => Some(Self::I32),
            "float" | "f32" => Some(Self::F32),
            "fourcc" => Some(Self::FourCC),
            _ => None,
        }
    }
}

impl Default for CustomUdpSpec {
    fn default() -> Self {
        Self {
            fields: builtin_mode_spec(0).fields,
        }
    }
}

impl CustomUdpSpec {
    pub fn from_mode(mode: u8) -> Self {
        match mode {
            0 => builtin_mode_spec(0),
            1 => builtin_mode_spec(1),
            2 => builtin_mode_spec(2),
            3 => builtin_mode_spec(3),
            _ => builtin_mode_spec(1),
        }
    }

    pub fn from_xml_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let raw = fs::read_to_string(path.as_ref()).with_context(|| {
            format!(
                "Failed to read custom UDP definition at {:?}",
                path.as_ref()
            )
        })?;
        parse_custom_udp_xml(&raw)
    }

    pub fn expected_bytes(&self) -> usize {
        self.fields.len() * FIELD_SIZE_BYTES
    }

    pub fn decode(&self, raw: &[u8]) -> Result<DecodedCodemastersPacket> {
        let expected = self.expected_bytes();
        if raw.len() < expected {
            return Err(anyhow!(
                "udp packet too short: expected at least {} bytes, got {} bytes",
                expected,
                raw.len()
            ));
        }

        let mut values = HashMap::with_capacity(self.fields.len());
        let mut fourcc = None;
        let mut offset = 0usize;

        for field in &self.fields {
            let chunk = &raw[offset..offset + FIELD_SIZE_BYTES];
            let bytes: [u8; FIELD_SIZE_BYTES] = chunk
                .try_into()
                .map_err(|_| anyhow!("failed to read 4-byte channel: {}", field.channel))?;
            let value = match field.field_type {
                FieldType::U32 => Some((u32::from_le_bytes(bytes) as f32) * field.scale),
                FieldType::I32 => Some((i32::from_le_bytes(bytes) as f32) * field.scale),
                FieldType::F32 => Some(f32::from_le_bytes(bytes) * field.scale),
                FieldType::FourCC => {
                    let text = String::from_utf8_lossy(&bytes)
                        .trim_end_matches('\0')
                        .trim()
                        .to_string();
                    fourcc = Some(text);
                    None
                }
            };

            if let Some(value) = value {
                values.insert(field.channel.clone(), value);
            }
            offset += FIELD_SIZE_BYTES;
        }

        Ok(DecodedCodemastersPacket { values, fourcc })
    }
}

pub(crate) fn canonical_channel_id(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace([' ', '-', '_'], "")
}

fn parse_custom_udp_xml(raw: &str) -> Result<CustomUdpSpec> {
    let mut reader = Reader::from_str(raw);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut fields = Vec::new();

    let mut active: Option<PendingField> = None;
    let mut in_field_depth: Option<usize> = None;
    let mut active_child: Option<String> = None;
    let mut depth = 0usize;

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| anyhow!("{e:?}"))?
        {
            Event::Start(element) => {
                depth += 1;
                let element_name = element.name();
                let tag = element_name.as_ref();

                if is_field_tag(tag) {
                    let field = parse_field_tag(&element)?;
                    if field.channel.is_some() && field.field_type.is_some() {
                        fields.push(complete_field(field));
                    } else {
                        in_field_depth = Some(depth);
                        active = Some(field);
                    }
                } else if active.is_some() {
                    let tag_name = tag_name(tag);
                    if matches!(tag_name, "name" | "type" | "scale" | "fourcc") {
                        active_child = Some(tag_name.to_string());
                    }
                }
            }
            Event::Empty(element) => {
                let element_name = element.name();
                let tag = element_name.as_ref();
                if is_field_tag(tag) {
                    let field = parse_field_tag(&element)?;
                    if let Some(completed) = finalize_field(field)? {
                        fields.push(completed);
                    }
                }
            }
            Event::Text(text) => {
                if let (Some(active_field), Some(child_name)) =
                    (active.as_mut(), active_child.as_deref())
                {
                    let text = std::str::from_utf8(text.as_ref())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if text.is_empty() {
                        continue;
                    }

                    match child_name {
                        "name" => active_field.pending_name = Some(text),
                        "type" => active_field.field_type = FieldType::parse(&text),
                        "scale" => {
                            active_field.scale = text
                                .parse::<f32>()
                                .ok()
                                .filter(|v| v.is_finite() && *v > 0.0);
                        }
                        "fourcc" => active_field.fourcc_text = Some(text),
                        _ => {}
                    }
                }
            }
            Event::End(end_tag) => {
                let end_name = end_tag.name();
                let tag = end_name.as_ref();
                let tag_name = tag_name(tag);

                if matches!(tag_name, "name" | "type" | "scale" | "fourcc") {
                    active_child = None;
                }

                if is_field_tag(tag) {
                    if in_field_depth == Some(depth)
                        && let Some(field) = active.take()
                        && let Some(completed) = finalize_field(field)?
                    {
                        fields.push(completed);
                    }
                    in_field_depth = None;
                }

                if let Some(current_depth) = in_field_depth
                    && depth == current_depth
                {
                    in_field_depth = None;
                }

                depth = depth.saturating_sub(1);
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }

    if fields.is_empty() {
        return Err(anyhow!("custom UDP XML did not produce any valid fields"));
    }

    Ok(CustomUdpSpec { fields })
}

fn parse_field_tag(element: &BytesStart<'_>) -> Result<PendingField> {
    let mut channel = None;
    let mut field_type = None;
    let mut scale = None;
    let mut fourcc_text = None;

    for attr in element.attributes().with_checks(false) {
        let attr = attr?;
        let key = attr.key.as_ref();
        let value = std::str::from_utf8(attr.value.as_ref())
            .unwrap_or("")
            .trim()
            .to_string();
        if value.is_empty() {
            continue;
        }

        match key {
            b"name" | b"channel" => channel = Some(canonical_channel_id(&value)),
            b"type" => field_type = FieldType::parse(&value),
            b"scale" => {
                scale = value
                    .parse::<f32>()
                    .ok()
                    .filter(|value| value.is_finite() && *value > 0.0);
            }
            b"fourcc" if !value.is_empty() => {
                fourcc_text = Some(value);
            }
            _ => {}
        }
    }

    Ok(PendingField {
        channel,
        field_type,
        scale,
        fourcc_text,
        pending_name: None,
    })
}

fn finalize_field(mut field: PendingField) -> Result<Option<FieldSpec>> {
    if let Some(pending_name) = field.pending_name.take().or_else(|| field.channel.clone()) {
        field.channel = Some(canonical_channel_id(&pending_name));
    }

    let channel = field.channel.take().ok_or_else(|| {
        anyhow!("custom UDP field is missing channel name in XML (set via name/channel attribute or <name> element)")
    })?;

    let field_type = field.field_type.ok_or_else(|| {
        anyhow!(
            "custom UDP field {:?} is missing type in XML (u32/i32/f32/fourcc)",
            channel
        )
    })?;

    let scale = field.scale.unwrap_or(1.0);
    Ok(Some(FieldSpec {
        channel,
        field_type,
        scale,
        fourcc_text: field.fourcc_text,
    }))
}

fn complete_field(field: PendingField) -> FieldSpec {
    // The caller already guarantees field completeness.
    let channel = field
        .channel
        .unwrap_or_else(|| "unnamed_channel".to_string());
    let field_type = field.field_type.unwrap_or(FieldType::F32);
    FieldSpec {
        channel,
        field_type,
        scale: field.scale.unwrap_or(1.0),
        fourcc_text: field.fourcc_text,
    }
}

fn is_field_tag(tag: &[u8]) -> bool {
    matches!(tag_name(tag), "field" | "channel")
}

fn tag_name(tag: &[u8]) -> &str {
    std::str::from_utf8(tag).unwrap_or("")
}

fn builtin_mode_spec(mode: u8) -> CustomUdpSpec {
    let mut fields = base_fields();
    match mode {
        1 => {
            fields.extend_from_slice(&[
                field("wheel_patch_speed_fl", FieldType::F32),
                field("wheel_patch_speed_fr", FieldType::F32),
                field("wheel_patch_speed_rl", FieldType::F32),
                field("wheel_patch_speed_rr", FieldType::F32),
                field("suspension_position_fl", FieldType::F32),
                field("suspension_position_fr", FieldType::F32),
                field("suspension_position_rl", FieldType::F32),
                field("suspension_position_rr", FieldType::F32),
            ]);
        }
        2 => {
            fields.extend_from_slice(&[
                field("wheel_patch_speed_fl", FieldType::F32),
                field("wheel_patch_speed_fr", FieldType::F32),
                field("wheel_patch_speed_rl", FieldType::F32),
                field("wheel_patch_speed_rr", FieldType::F32),
                field("suspension_velocity_fl", FieldType::F32),
                field("suspension_velocity_fr", FieldType::F32),
                field("suspension_velocity_rl", FieldType::F32),
                field("suspension_velocity_rr", FieldType::F32),
                field("long_accel", FieldType::F32),
                field("lat_accel", FieldType::F32),
            ]);
        }
        3 => {
            fields.extend_from_slice(&[
                field("wheel_patch_speed_fl", FieldType::F32),
                field("wheel_patch_speed_fr", FieldType::F32),
                field("wheel_patch_speed_rl", FieldType::F32),
                field("wheel_patch_speed_rr", FieldType::F32),
                field("suspension_velocity_fl", FieldType::F32),
                field("suspension_velocity_fr", FieldType::F32),
                field("suspension_velocity_rl", FieldType::F32),
                field("suspension_velocity_rr", FieldType::F32),
                field("suspension_position_fl", FieldType::F32),
                field("suspension_position_fr", FieldType::F32),
                field("suspension_position_rl", FieldType::F32),
                field("suspension_position_rr", FieldType::F32),
                field("long_accel", FieldType::F32),
                field("lat_accel", FieldType::F32),
                field("vert_accel", FieldType::F32),
            ]);
        }
        _ => {}
    }

    CustomUdpSpec { fields }
}

fn base_fields() -> Vec<FieldSpec> {
    vec![
        field("speed", FieldType::F32),
        field("engine_rate", FieldType::F32),
        field("gear", FieldType::I32),
        field("steering_input", FieldType::F32),
        field("throttle_input", FieldType::F32),
        field("brake_input", FieldType::F32),
        field("clutch_input", FieldType::F32),
    ]
}

fn field(name: &str, field_type: FieldType) -> FieldSpec {
    FieldSpec {
        channel: canonical_channel_id(name),
        field_type,
        scale: 1.0,
        fourcc_text: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn parse_and_decode_builtin_mode_fields() -> Result<()> {
        let spec = CustomUdpSpec::from_mode(1);
        let mut packet = Vec::new();

        packet.extend_from_slice(&0.0f32.to_le_bytes()); // speed
        packet.extend_from_slice(&(120.0f32.to_le_bytes())); // engine_rate
        packet.extend_from_slice(&(4i32.to_le_bytes())); // gear
        packet.extend_from_slice(&(0.05f32.to_le_bytes())); // steering
        packet.extend_from_slice(&(0.75f32.to_le_bytes())); // throttle
        packet.extend_from_slice(&(0.2f32.to_le_bytes())); // brake
        packet.extend_from_slice(&(0.0f32.to_le_bytes())); // clutch
        packet.extend_from_slice(&12.3f32.to_le_bytes()); // patch fl
        packet.extend_from_slice(&11.8f32.to_le_bytes()); // patch fr
        packet.extend_from_slice(&11.9f32.to_le_bytes()); // patch rl
        packet.extend_from_slice(&12.2f32.to_le_bytes()); // patch rr
        packet.extend_from_slice(&0.1f32.to_le_bytes()); // suspension fl
        packet.extend_from_slice(&0.1f32.to_le_bytes()); // suspension fr
        packet.extend_from_slice(&0.1f32.to_le_bytes()); // suspension rl
        packet.extend_from_slice(&0.1f32.to_le_bytes()); // suspension rr

        let decoded = spec.decode(&packet)?;
        assert_eq!(decoded.values.get("speed"), Some(&0.0));
        assert_eq!(decoded.values.get("enginerate"), Some(&120.0));
        assert_eq!(decoded.values.get("gear"), Some(&4.0));
        Ok(())
    }

    #[test]
    fn parse_xml_definition_with_attributes() -> Result<()> {
        let xml = r#"
        <custom_udp>
            <field name="speed" type="float" />
            <field name="engine rate" type="f32" />
            <field name="fourcc" type="fourcc" />
        </custom_udp>
        "#;
        let spec = parse_custom_udp_xml(xml)?;

        assert_eq!(spec.fields.len(), 3);
        let mut packet = Vec::new();
        packet.extend_from_slice(&20.0f32.to_le_bytes());
        packet.extend_from_slice(&90.0f32.to_le_bytes());
        packet.extend_from_slice(b"ABCD");

        let decoded = spec.decode(&packet)?;
        assert_eq!(decoded.fourcc.as_deref(), Some("ABCD"));
        assert_eq!(decoded.values.get("speed"), Some(&20.0));
        assert_eq!(decoded.values.get("enginerate"), Some(&90.0));
        Ok(())
    }

    proptest! {
        #[test]
        fn prop_decode_errors_on_short_input(data: Vec<u8>) {
            let spec = CustomUdpSpec::from_mode(0);
            let expected = spec.expected_bytes();
            if data.len() < expected {
                prop_assert!(spec.decode(&data).is_err());
            } else {
                prop_assert!(spec.decode(&data).is_ok());
            }
        }

        #[test]
        fn prop_decode_value_count_matches_non_fourcc_fields(data: Vec<u8>) {
            let spec = CustomUdpSpec::from_mode(0);
            let non_fourcc_count = spec.fields.iter().filter(|f| !matches!(f.field_type, FieldType::FourCC)).count();
            if let Ok(decoded) = spec.decode(&data) {
                prop_assert_eq!(decoded.values.len(), non_fourcc_count);
            }
        }
    }
}
