use glam::Vec3;
use nom::{
    branch::alt,
    bytes::complete::{tag, take_while, take_while1},
    character::complete::char,
    combinator::value,
    multi::many1,
    number::complete::float,
    sequence::preceded,
    IResult,
};

struct LookAt {
    eye: Vec3,
    look_at: Vec3,
    up: Vec3,
}

fn comment(input: &str) -> IResult<&str, &str> {
    let (rest, _) = char('#')(input)?;
    take_while(|c| c != '\n')(rest)
}

fn space(input: &str) -> IResult<&str, &str> {
    let chars = " \t\r\n";

    take_while1(move |c| chars.contains(c))(input)
}

fn sp(input: &str) -> IResult<&str, ()> {
    value((), many1(alt((space, comment))))(input)
}

fn parse_vec3(input: &str) -> IResult<&str, Vec3> {
    let (rest, x1) = preceded(sp, float)(input)?;
    let (rest, x2) = preceded(sp, float)(rest)?;
    let (rest, x3) = preceded(sp, float)(rest)?;

    Ok((rest, Vec3::new(x1, x2, x3)))
}

fn parse_look_at(input: &str) -> IResult<&str, LookAt> {
    let (rest, _) = tag("LookAt")(input)?;
    let (rest, eye) = parse_vec3(rest)?;
    let (rest, look_at) = parse_vec3(rest)?;
    let (rest, up) = parse_vec3(rest)?;

    Ok((rest, LookAt { eye, look_at, up }))
}

#[cfg(test)]
mod test {
    use approx::abs_diff_eq;

    use super::*;

    #[test]
    fn test_parse_space() {
        assert_eq!(space("    "), Ok(("", "    ")));
    }

    #[test]
    fn test_parse_comment() {
        assert_eq!(comment("#Hello"), Ok(("", "Hello")));
    }

    #[test]
    fn test_parse_sp() {
        assert_eq!(sp("    "), Ok(("", ())));
    }

    #[test]
    fn test_parse_look_at() {
        let (_, look_at) = parse_look_at(
            r#"LookAt 3 4 1.5  # eye
                            .5 .5 0  # look at point
                            0 0 1    # up vector"#,
        )
        .unwrap();

        assert!(abs_diff_eq!(look_at.eye.to_array()[..], [3.0, 4.0, 1.5]));
        assert!(abs_diff_eq!(
            look_at.look_at.to_array()[..],
            [0.5, 0.5, 0.0]
        ));
        assert!(abs_diff_eq!(look_at.up.to_array()[..], [0.0, 0.0, 1.0]));
    }
}
