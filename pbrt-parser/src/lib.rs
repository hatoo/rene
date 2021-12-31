use glam::{vec3a, Vec3A};
use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_while, take_while1},
    character::complete::{alphanumeric1, char, digit1, one_of},
    combinator::{cut, map, map_res, opt, recognize, value},
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
    Translate(Vec3A),
}

#[derive(PartialEq, Debug)]
pub struct LookAt {
    pub eye: Vec3A,
    pub look_at: Vec3A,
    pub up: Vec3A,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Float(Vec<f32>),
    Integer(Vec<i32>),
    Rgb(Vec<f32>),
}

#[derive(PartialEq, Debug)]
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
#[derive(PartialEq, Debug)]
pub struct Object<'a, T> {
    pub object_type: T,
    pub t: &'a str,
    pub arguments: Vec<Argument<'a>>,
}

pub type SceneObject<'a> = Object<'a, SceneObjectType>;
pub type WorldObject<'a> = Object<'a, WorldObjectType>;

#[derive(thiserror::Error, Debug)]
pub enum ArgumentError {
    #[error("unmatched value length")]
    UnmatchedValueLength,
    #[error("unmatched type")]
    UnmatchedType,
}

impl<'a, T> Object<'a, T> {
    pub fn get_rgb(&self, name: &str) -> Option<Result<Vec3A, ArgumentError>> {
        self.arguments
            .iter()
            .find(|a| a.name == name)
            .map(|a| match &a.value {
                Value::Rgb(v) => {
                    if v.len() == 3 {
                        Ok(vec3a(v[0], v[1], v[2]))
                    } else {
                        Err(ArgumentError::UnmatchedValueLength)
                    }
                }
                _ => Err(ArgumentError::UnmatchedType),
            })
    }

    pub fn get_float(&self, name: &str) -> Option<Result<f32, ArgumentError>> {
        self.arguments
            .iter()
            .find(|a| a.name == name)
            .map(|a| match &a.value {
                Value::Float(v) => {
                    if v.len() == 1 {
                        Ok(v[0])
                    } else {
                        Err(ArgumentError::UnmatchedValueLength)
                    }
                }
                _ => Err(ArgumentError::UnmatchedType),
            })
    }

    pub fn get_integers(&self, name: &str) -> Option<Result<&[i32], ArgumentError>> {
        self.arguments
            .iter()
            .find(|a| a.name == name)
            .map(|a| match &a.value {
                Value::Integer(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType),
            })
    }

    pub fn get_floats(&self, name: &str) -> Option<Result<&[f32], ArgumentError>> {
        self.arguments
            .iter()
            .find(|a| a.name == name)
            .map(|a| match &a.value {
                Value::Float(v) => Ok(v.as_slice()),
                _ => Err(ArgumentError::UnmatchedType),
            })
    }
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

fn parse_vec3(input: &str) -> IResult<&str, Vec3A> {
    let (rest, x1) = preceded(sp, float)(input)?;
    let (rest, x2) = preceded(sp, float)(rest)?;
    let (rest, x3) = preceded(sp, float)(rest)?;

    Ok((rest, Vec3A::new(x1, x2, x3)))
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
    Integer,
}

fn parse_argument_type(input: &str) -> IResult<&str, ArgumentType> {
    alt((
        value(ArgumentType::Float, tag("float")),
        value(ArgumentType::Integer, tag("integer")),
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

fn integer(input: &str) -> IResult<&str, i32> {
    fn plus(i: &str) -> IResult<&str, i32> {
        map_res(recognize(digit1), str::parse)(i)
    }
    fn minus(i: &str) -> IResult<&str, i32> {
        let (rest, _) = char('-')(i)?;
        plus(rest)
    }

    alt((plus, minus))(input)
}

fn integers(input: &str) -> IResult<&str, Vec<i32>> {
    alt((map(integer, |f| vec![f]), |i| bracket(integer, i)))(input)
}

impl ArgumentType {
    fn parse_value(self, input: &str) -> IResult<&str, Value> {
        match self {
            ArgumentType::Float => floats(input).map(|(rest, f)| (rest, Value::Float(f))),
            ArgumentType::Integer => integers(input).map(|(rest, f)| (rest, Value::Integer(f))),
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

fn parse_transrate(input: &str) -> IResult<&str, Vec3A> {
    let (rest, _) = tag("Translate")(input)?;
    preceded(sp, parse_vec3)(rest)
}

fn parse_world(input: &str) -> IResult<&str, World> {
    alt((
        map(parse_world_object, |w| World::WorldObject(w)),
        map(parse_attribute_statement, |w| World::Attribute(w)),
        map(parse_transrate, |v| World::Translate(v)),
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
        assert_eq!(
            LookAt {
                eye: vec3a(3.0, 4.0, 1.5),
                look_at: vec3a(0.5, 0.5, 0.0),
                up: vec3a(0.0, 0.0, 1.0)
            },
            parse_look_at(
                r#"LookAt 3 4 1.5  # eye
                            .5 .5 0  # look at point
                            0 0 1    # up vector"#,
            )
            .unwrap()
            .1
        );
    }

    #[test]
    fn test_parse_scene_object() {
        for q in [
            r#"Camera "perspective" "float fov" 45"#,
            r#"Camera "perspective" "float fov" [45]"#,
        ] {
            assert_eq!(
                (
                    "",
                    SceneObject {
                        object_type: SceneObjectType::Camera,
                        t: "perspective",
                        arguments: vec![Argument {
                            name: "fov",
                            value: Value::Float(vec![45.0])
                        }]
                    }
                ),
                parse_scene_object(q).unwrap()
            );
        }
    }

    #[test]
    fn test_parse_world_object() {
        assert_eq!(
            (
                "",
                WorldObject {
                    object_type: WorldObjectType::LightSource,
                    t: "infinite",
                    arguments: vec![Argument {
                        name: "L",
                        value: Value::Rgb(vec![0.4, 0.45, 0.5])
                    }]
                }
            ),
            parse_world_object(r#"LightSource "infinite" "rgb L" [.4 .45 .5]"#).unwrap()
        );
    }

    #[test]
    fn test_parse_pbrt() {
        parse_pbrt(
            r#"
        LookAt 3 4 1.5  # eye
            .0 .0 0  # look at point
            0 0 1    # up vector
        Camera "perspective" "float fov" 45

        WorldBegin

        # uniform blue-ish illumination from all directions
        LightSource "infinite" "rgb L" [.4 .45 .5]

        AttributeBegin
        Material "matte" "rgb Kd" [ .7 .2 .2 ]
        Shape "sphere" "float radius" 1
        AttributeEnd

        WorldEnd
        "#,
        )
        .unwrap();
    }
}
