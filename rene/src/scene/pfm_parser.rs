use nom::{
    bytes::complete::{tag, take_while},
    character::is_digit,
    number::complete::{be_f32, float, le_f32},
    IResult,
};

use super::image::Image;

pub fn parse_pfm_rgb(input: &[u8]) -> IResult<&[u8], Image> {
    let (rest, _) = tag("PF\n")(input)?;
    let (rest, width) = take_while(is_digit)(rest)?;
    let (rest, _) = tag(" ")(rest)?;
    let (rest, height) = take_while(is_digit)(rest)?;
    let (rest, _) = tag("\n")(rest)?;
    let (rest, order) = float(rest)?;
    let (rest, _) = tag("\n")(rest)?;

    let width: u32 = width
        .iter()
        .map(|&b| b as char)
        .collect::<String>()
        .parse()
        .unwrap();

    let height: u32 = height
        .iter()
        .map(|&b| b as char)
        .collect::<String>()
        .parse()
        .unwrap();

    let mut data = vec![[0.0; 4]; (width * height) as usize];

    let mut rest = rest;

    for y in (0..height).rev() {
        for x in 0..width {
            let (rgb, r) = if order > 0.0 {
                let (rest, r) = be_f32(rest)?;
                let (rest, g) = be_f32(rest)?;
                let (rest, b) = be_f32(rest)?;

                ([r, g, b], rest)
            } else {
                let (rest, r) = le_f32(rest)?;
                let (rest, g) = le_f32(rest)?;
                let (rest, b) = le_f32(rest)?;

                ([r, g, b], rest)
            };

            rest = r;

            data[(y * width + x) as usize][..3].copy_from_slice(rgb.as_slice());
            data[(y * width + x) as usize][3] = 1.0;
        }
    }

    Ok((
        rest,
        Image {
            width,
            height,
            data,
        },
    ))
}
