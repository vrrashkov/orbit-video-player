pub struct ColorSpaceConfig {
    pub matrix: [[f32; 3]; 3],
    pub y_range: [f32; 2],
    pub uv_range: [f32; 2],
}

pub const BT709_CONFIG: ColorSpaceConfig = ColorSpaceConfig {
    matrix: [
        [1.0, 0.0, 1.5748],
        [1.0, -0.1873, -0.4681],
        [1.0, 1.8556, 0.0],
    ],
    y_range: [16.0 / 255.0, 235.0 / 255.0],
    uv_range: [16.0 / 255.0, 240.0 / 255.0],
};
