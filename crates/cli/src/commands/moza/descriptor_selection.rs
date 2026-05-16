use anyhow::{Result, anyhow};

use super::{MozaDeviceRecord, report_descriptor_from_operator_hex, selector_matches};

pub(super) fn apply_operator_report_descriptor_to_selected_device(
    devices: &mut [MozaDeviceRecord],
    selector: Option<&str>,
    report_descriptor_hex: &str,
) -> Result<()> {
    let selected_indices: Vec<_> = devices
        .iter()
        .enumerate()
        .filter_map(|(index, device)| selector_matches(device, selector).then_some(index))
        .collect();

    if selected_indices.len() != 1 {
        return Err(anyhow!(
            "operator-supplied report descriptor requires exactly one selected Moza HID device, found {}",
            selected_indices.len()
        ));
    }

    let descriptor = report_descriptor_from_operator_hex(report_descriptor_hex)?;
    if let Some(device) = selected_indices
        .first()
        .and_then(|index| devices.get_mut(*index))
    {
        device.apply_report_descriptor(descriptor, "operator_supplied_hex");
        Ok(())
    } else {
        Err(anyhow!(
            "operator-supplied report descriptor selected device disappeared before descriptor metadata could be applied"
        ))
    }
}
