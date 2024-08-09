pub fn freq_to_midi(frequency: f32) -> f32 {
    69.0 + 12.0 * (frequency/440.0).log2()
}

pub fn scale(
    input:            f32,
    min_input_value:  f32,
    max_input_value:  f32,
    min_output_value: f32,
    max_output_value: f32,
) -> f32 {
    let normalized = (input - min_input_value) / (max_input_value - min_input_value);
    normalized * max_output_value + (1.0 - normalized) * min_output_value
}

pub fn limit_f32(
    input:     f32,
    min_value: f32,
    max_value: f32,
) -> f32 {
    f32::min(max_value, f32::max(min_value, input))
}

pub fn limit_u8(
    input:     u8,
    min_value: u8,
    max_value: u8,
) -> u8 {
    u8::min(max_value, u8::max(min_value, input))
}
