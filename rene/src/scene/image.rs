#[derive(Debug, Clone)]
// deny manual construct
#[non_exhaustive]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub data: Vec<[f32; 4]>,
}

impl Image {
    pub fn new(width: u32, height: u32, data: Vec<[f32; 4]>) -> Self {
        assert_eq!((width * height) as usize, data.len());
        Self {
            width,
            height,
            data,
        }
    }
}
