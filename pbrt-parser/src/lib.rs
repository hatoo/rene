use glam::Vec3;
use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_while, take_while1},
    character::complete::{alphanumeric1, char, one_of},
    combinator::{cut, map, opt, value},
    error::{Error, ErrorKind},
    multi::many0,
    number::complete::float,
    sequence::{preceded, terminated},
    AsChar, Err, IResult,
};

pub enum Scene<'a> {
    LookAt(LookAt),
    SceneObject(SceneObject<'a>),
    World(Vec<World<'a>>),
}

pub enum World<'a> {
    WorldObject(WorldObject<'a>),
    Attribute(Vec<World<'a>>),
}

pub struct LookAt {
    pub eye: Vec3,
    pub look_at: Vec3,
    pub up: Vec3,
}

#[derive(Clone, Debug)]
pub enum Value {
    Float(Vec<f32>),
    Rgb(Vec<f32>),
}

pub struct Argument<'a> {
    pub name: &'a str,
    pub value: Value,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SceneObjectType {
    Camera,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorldObjectType {
    LightSource,
    Material,
    Shape,
}

pub struct SceneObject<'a> {
    pub object_type: SceneObjectType,
    pub t: &'a str,
    pub arguments: Vec<Argument<'a>>,
}

pub struct WorldObject<'a> {
    pub object_type: WorldObjectType,
    pub t: &'a str,
    pub arguments: Vec<Argument<'a>>,
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
    value((), many0(alt((space, comment))))(input)
}

fn parse_vec3(input: &str) -> IResult<&str, Vec3> {
    let (rest, x1) = preceded(sp, float)(input)?;
    let (rest, x2) = preceded(sp, float)(rest)?;
    let (rest, x3) = preceded(sp, float)(rest)?;

    Ok((rest, Vec3::new(x1, x2, x3)))
}

fn parse_str(i: &str) -> IResult<&str, &str> {
    fn parse(i: &str) -> IResult<&str, &str> {
        escaped(alphanumeric1, '\\', one_of("\"n\\"))(i)
    }
    preceded(char('\"'), cut(terminated(parse, char('\"'))))(i)
}

#[derive(Clone, Copy, Debug)]
enum ArgumentType {
    Float,
    Rgb,
}

fn parse_argument_type(input: &str) -> IResult<&str, ArgumentType> {
    alt((
        value(ArgumentType::Float, tag("float")),
        value(ArgumentType::Rgb, alt((tag("rgb"), tag("color")))),
    ))(input)
}

fn bracket<'a, T: Clone, F: Fn(&'a str) -> IResult<&'a str, T>>(
    p: F,
    input: &'a str,
) -> IResult<&'a str, Vec<T>> {
    let (rest, _) = char('[')(input)?;
    let (rest, v) = many0(preceded(sp, p))(rest)?;
    value(v, preceded(sp, char(']')))(rest)
}

fn floats(input: &str) -> IResult<&str, Vec<f32>> {
    alt((map(float, |f| vec![f]), |i| bracket(float, i)))(input)
}

impl ArgumentType {
    fn parse_value(self, input: &str) -> IResult<&str, Value> {
        match self {
            ArgumentType::Float => floats(input).map(|(rest, f)| (rest, Value::Float(f))),
            ArgumentType::Rgb => bracket(&float, input).map(|(rest, v)| (rest, Value::Rgb(v))),
        }
    }
}

fn parse_argument_type_name(input: &str) -> IResult<&str, (ArgumentType, &str)> {
    fn parse(input: &str) -> IResult<&str, (ArgumentType, &str)> {
        let (rest, ty) = parse_argument_type(input)?;
        let (rest, _) = char(' ')(rest)?;
        let (rest, ident) = take_while(|c: char| c.is_alphanum())(rest)?;
        Ok((rest, (ty, ident)))
    }
    preceded(char('\"'), cut(terminated(parse, char('\"'))))(input)
}

fn parse_argument(input: &str) -> IResult<&str, Argument> {
    let (rest, (ty, name)) = parse_argument_type_name(input)?;
    let (rest, value) = preceded(sp, |i| ty.parse_value(i))(rest)?;

    Ok((rest, Argument { name, value }))
}

fn parse_look_at(input: &str) -> IResult<&str, LookAt> {
    let (rest, _) = tag("LookAt")(input)?;
    let (rest, eye) = parse_vec3(rest)?;
    let (rest, look_at) = parse_vec3(rest)?;
    let (rest, up) = parse_vec3(rest)?;

    Ok((rest, LookAt { eye, look_at, up }))
}

fn parse_scene_object_type(input: &str) -> IResult<&str, SceneObjectType> {
    alt((value(SceneObjectType::Camera, tag("Camera")),))(input)
}

fn parse_world_object_type(input: &str) -> IResult<&str, WorldObjectType> {
    alt((
        value(WorldObjectType::LightSource, tag("LightSource")),
        value(WorldObjectType::Material, tag("Material")),
        value(WorldObjectType::Shape, tag("Shape")),
    ))(input)
}

fn parse_scene_object(input: &str) -> IResult<&str, SceneObject> {
    let (rest, ty) = parse_scene_object_type(input)?;
    let (rest, t) = preceded(sp, parse_str)(rest)?;
    let (rest, arguments) = preceded(sp, many0(parse_argument))(rest)?;

    Ok((
        rest,
        SceneObject {
            object_type: ty,
            t,
            arguments,
        },
    ))
}

fn parse_world_object(input: &str) -> IResult<&str, WorldObject> {
    let (rest, ty) = parse_world_object_type(input)?;
    let (rest, t) = preceded(sp, parse_str)(rest)?;
    let (rest, arguments) = preceded(sp, many0(parse_argument))(rest)?;

    Ok((
        rest,
        WorldObject {
            object_type: ty,
            t,
            arguments,
        },
    ))
}

fn parse_attribute_statement(input: &str) -> IResult<&str, Vec<World>> {
    let (rest, _) = tag("AttributeBegin")(input)?;
    let (rest, worlds) = many0(preceded(sp, parse_world))(rest)?;
    let (rest, _) = preceded(sp, tag("AttributeEnd"))(rest)?;

    Ok((rest, worlds))
}

fn parse_world(input: &str) -> IResult<&str, World> {
    alt((
        map(parse_world_object, |w| World::WorldObject(w)),
        map(parse_attribute_statement, |w| World::Attribute(w)),
    ))(input)
}

fn parse_world_statement(input: &str) -> IResult<&str, Vec<World>> {
    let (rest, _) = tag("WorldBegin")(input)?;
    let (rest, worlds) = many0(preceded(sp, parse_world))(rest)?;
    let (rest, _) = preceded(sp, tag("WorldEnd"))(rest)?;

    Ok((rest, worlds))
}

fn parse_scene(input: &str) -> IResult<&str, Vec<Scene>> {
    many0(preceded(
        sp,
        alt((
            map(parse_look_at, |l| Scene::LookAt(l)),
            map(parse_scene_object, |o| Scene::SceneObject(o)),
            map(parse_world_statement, |w| Scene::World(w)),
        )),
    ))(input)
}

pub fn parse_pbrt(input: &str) -> Result<Vec<Scene>, Err<Error<&str>>> {
    let (rest, scene) = parse_scene(input)?;
    let (rest, _) = opt(sp)(rest)?;

    if rest != "" {
        Err(Err::Error(Error::new(rest, ErrorKind::Fail)))
    } else {
        Ok(scene)
    }
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
    fn test_sp() {
        assert_eq!(sp("    # aaaaa"), Ok(("", ())));
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

    #[test]
    fn test_parse_scene_object() {
        for q in [
            r#"Camera "perspective" "float fov" 45"#,
            r#"Camera "perspective" "float fov" [45]"#,
        ] {
            let (rest, camera) = parse_scene_object(q).unwrap();
            assert_eq!(rest, "");

            assert_eq!(camera.object_type, SceneObjectType::Camera);
            assert_eq!(camera.t, "perspective");
            assert_eq!(camera.arguments.len(), 1);
            assert_eq!(camera.arguments[0].name, "fov");
            match camera.arguments[0].value {
                Value::Float(ref f) => assert!(abs_diff_eq!(f[..], [45.0])),
                _ => panic!(),
            }
        }
    }

    #[test]
    fn test_parse_world_object() {
        let (rest, light_source) =
            parse_world_object(r#"LightSource "infinite" "rgb L" [.4 .45 .5]"#).unwrap();
        assert_eq!(rest, "");

        assert_eq!(light_source.object_type, WorldObjectType::LightSource);
        assert_eq!(light_source.t, "infinite");
        assert_eq!(light_source.arguments.len(), 1);
        assert_eq!(light_source.arguments[0].name, "L");
        match light_source.arguments[0].value {
            Value::Rgb(ref v) => assert!(abs_diff_eq!(v[..], [0.4, 0.45, 0.5])),
            _ => panic!(),
        }
    }
}
