use glam::{vec3a, Mat4, Vec3A, Vec4};
use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_while, take_while1},
    character::complete::{char, digit1, none_of, one_of},
    combinator::{cut, eof, map, recognize, value},
    error::{Error, ParseError},
    multi::many0,
    number::complete::float,
    sequence::{preceded, terminated},
    AsChar, Finish, IResult,
};

pub mod include;

pub enum Scene<'a> {
    Transform(Mat4),
    LookAt(LookAt),
    Rotate(AxisAngle),
    SceneObject(SceneObject<'a>),
    World(Vec<World<'a>>),
}

pub struct AxisAngle {
    pub axis: Vec3A,
    pub angle: f32,
}

pub struct Texture<'a> {
    pub name: &'a str,
    pub value_type: &'a str,
    pub obj: Object<'a, ()>,
}

pub enum World<'a> {
    WorldObject(WorldObject<'a>),
    Attribute(Vec<World<'a>>),
    TransformBeginEnd(Vec<World<'a>>),
    Transform(Mat4),
    Translate(Vec3A),
    Scale(Vec3A),
    Rotate(AxisAngle),
    Texture(Texture<'a>),
    NamedMaterial(&'a str),
}

#[derive(PartialEq, Debug)]
pub struct LookAt {
    pub eye: Vec3A,
    pub look_at: Vec3A,
    pub up: Vec3A,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Value<'a> {
    Float(Vec<f32>),
    Integer(Vec<i32>),
    Rgb(Vec<f32>),
    BlackBody(Vec<f32>),
    Point(Vec<Vec3A>),
    Normal(Vec<Vec3A>),
    String(Vec<&'a str>),
    Texture(Vec<&'a str>),
}

#[derive(PartialEq, Debug, Clone)]
pub struct Argument<'a> {
    pub name: &'a str,
    pub value: Value<'a>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SceneObjectType {
    Camera,
    Sampler,
    Integrator,
    PixelFilter,
    Film,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WorldObjectType {
    LightSource,
    AreaLightSource,
    Material,
    MakeNamedMaterial,
    Shape,
}
#[derive(PartialEq, Debug, Clone)]
pub struct Object<'a, T> {
    pub object_type: T,
    pub t: &'a str,
    pub arguments: Vec<Argument<'a>>,
}

pub type SceneObject<'a> = Object<'a, SceneObjectType>;
pub type WorldObject<'a> = Object<'a, WorldObjectType>;

impl<'a, T> Object<'a, T> {
    pub fn get_value(&self, name: &str) -> Option<&Value> {
        self.arguments
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.value)
    }
}

fn comment<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    let (rest, _) = char('#')(input)?;
    take_while(|c| c != '\n')(rest)
}

fn space<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    let chars = " \t\r\n";

    take_while1(move |c| chars.contains(c))(input)
}

pub(crate) fn sp<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, (), E> {
    value((), many0(alt((space, comment))))(input)
}

fn parse_vec3<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Vec3A, E> {
    let (rest, x1) = preceded(sp, float)(input)?;
    let (rest, x2) = preceded(sp, float)(rest)?;
    let (rest, x3) = preceded(sp, float)(rest)?;

    Ok((rest, Vec3A::new(x1, x2, x3)))
}

fn parse_vec4<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Vec4, E> {
    let (rest, x1) = preceded(sp, float)(input)?;
    let (rest, x2) = preceded(sp, float)(rest)?;
    let (rest, x3) = preceded(sp, float)(rest)?;
    let (rest, x4) = preceded(sp, float)(rest)?;

    Ok((rest, Vec4::new(x1, x2, x3, x4)))
}

fn parse_axis_angle<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, AxisAngle, E> {
    let (rest, a) = preceded(sp, float)(input)?;
    let (rest, x) = preceded(sp, float)(rest)?;
    let (rest, y) = preceded(sp, float)(rest)?;
    let (rest, z) = preceded(sp, float)(rest)?;

    Ok((
        rest,
        AxisAngle {
            axis: vec3a(x, y, z),
            angle: a,
        },
    ))
}

pub(crate) fn parse_str<'a, E: ParseError<&'a str>>(i: &'a str) -> IResult<&'a str, &'a str, E> {
    fn parse<'a, E: ParseError<&'a str>>(i: &'a str) -> IResult<&'a str, &'a str, E> {
        escaped(none_of("\""), '\\', one_of("\"n\\"))(i)
    }
    preceded(char('\"'), cut(terminated(parse, char('\"'))))(i)
}

#[derive(Clone, Copy, Debug)]
enum ArgumentType {
    Float,
    Rgb,
    BlackBody,
    Integer,
    Point,
    Normal,
    String,
    Texture,
}

fn parse_argument_type<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, ArgumentType, E> {
    alt((
        value(ArgumentType::Float, tag("float")),
        value(ArgumentType::Integer, tag("integer")),
        value(ArgumentType::String, tag("string")),
        value(ArgumentType::Point, tag("point")),
        value(ArgumentType::Normal, tag("normal")),
        value(ArgumentType::Texture, tag("texture")),
        value(ArgumentType::BlackBody, tag("blackbody")),
        value(ArgumentType::Rgb, alt((tag("rgb"), tag("color")))),
    ))(input)
}

fn bracket<'a, T: Clone, E: ParseError<&'a str>, F: Fn(&'a str) -> IResult<&'a str, T, E>>(
    p: F,
    input: &'a str,
) -> IResult<&'a str, Vec<T>, E> {
    let (rest, _) = char('[')(input)?;
    let (rest, v) = many0(preceded(sp, p))(rest)?;
    value(v, preceded(sp, char(']')))(rest)
}

fn strs<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Vec<&'a str>, E> {
    alt((map(parse_str, |s| vec![s]), |i| bracket(parse_str, i)))(input)
}

fn floats<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Vec<f32>, E> {
    alt((map(float, |f| vec![f]), |i| bracket(float, i)))(input)
}

fn integer<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, i32, E> {
    fn plus<'a, E: ParseError<&'a str>>(i: &'a str) -> IResult<&'a str, i32, E> {
        map(recognize(digit1), |i| str::parse(i).unwrap())(i)
    }
    fn minus<'a, E: ParseError<&'a str>>(i: &'a str) -> IResult<&'a str, i32, E> {
        let (rest, _) = char('-')(i)?;
        map(plus, |i| -i)(rest)
    }

    alt((plus, minus))(input)
}

fn integers<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Vec<i32>, E> {
    alt((map(integer, |f| vec![f]), |i| bracket(integer, i)))(input)
}

impl ArgumentType {
    fn parse_value<'a, E: ParseError<&'a str>>(self, input: &'a str) -> IResult<&'a str, Value, E> {
        match self {
            ArgumentType::Float => floats(input).map(|(rest, f)| (rest, Value::Float(f))),
            ArgumentType::Point => {
                let (rest, fs) = floats(input)?;
                /*
                if fs.len() % 3 != 0 {
                    return Err(nom::Err::Error(nom::error::Error::new(
                        input,
                        nom::error::ErrorKind::Many0,
                    )));
                }
                */

                Ok((
                    rest,
                    Value::Point(fs.chunks(3).map(|v| vec3a(v[0], v[1], v[2])).collect()),
                ))
            }
            ArgumentType::Normal => {
                let (rest, fs) = floats(input)?;
                /*
                if fs.len() % 3 != 0 {
                    return Err(nom::Err::Error(nom::error::Error::new(
                        input,
                        nom::error::ErrorKind::Many0,
                    )));
                }
                */

                Ok((
                    rest,
                    Value::Normal(fs.chunks(3).map(|v| vec3a(v[0], v[1], v[2])).collect()),
                ))
            }
            ArgumentType::String => map(strs, Value::String)(input),
            ArgumentType::Texture => map(strs, Value::Texture)(input),
            ArgumentType::Integer => integers(input).map(|(rest, f)| (rest, Value::Integer(f))),
            ArgumentType::Rgb => bracket(&float, input).map(|(rest, v)| (rest, Value::Rgb(v))),
            ArgumentType::BlackBody => {
                bracket(&float, input).map(|(rest, v)| (rest, Value::BlackBody(v)))
            }
        }
    }
}

fn parse_argument_type_name<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, (ArgumentType, &'a str), E> {
    fn parse<'a, E: ParseError<&'a str>>(
        input: &'a str,
    ) -> IResult<&'a str, (ArgumentType, &'a str), E> {
        let (rest, ty) = parse_argument_type(input)?;
        let (rest, _) = char(' ')(rest)?;
        let (rest, ident) = take_while(|c: char| c.is_alphanum())(rest)?;
        Ok((rest, (ty, ident)))
    }
    preceded(char('\"'), cut(terminated(parse, char('\"'))))(input)
}

fn parse_argument<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Argument, E> {
    let (rest, (ty, name)) = parse_argument_type_name(input)?;
    let (rest, value) = preceded(sp, |i| ty.parse_value(i))(rest)?;

    Ok((rest, Argument { name, value }))
}

fn parse_look_at<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, LookAt, E> {
    let (rest, _) = tag("LookAt")(input)?;
    let (rest, eye) = parse_vec3(rest)?;
    let (rest, look_at) = parse_vec3(rest)?;
    let (rest, up) = parse_vec3(rest)?;

    Ok((rest, LookAt { eye, look_at, up }))
}

fn parse_transform<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Mat4, E> {
    let (rest, _) = tag("Transform")(input)?;
    let (rest, _) = preceded(sp, char('['))(rest)?;
    let (rest, x) = parse_vec4(rest)?;
    let (rest, y) = parse_vec4(rest)?;
    let (rest, z) = parse_vec4(rest)?;
    let (rest, w) = parse_vec4(rest)?;
    let (rest, _) = preceded(sp, char(']'))(rest)?;

    Ok((rest, Mat4::from_cols(x, y, z, w)))
}

fn parse_named_material<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, &'a str, E> {
    let (rest, _) = tag("NamedMaterial")(input)?;
    preceded(sp, parse_str)(rest)
}

fn parse_scene_object_type<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, SceneObjectType, E> {
    alt((
        value(SceneObjectType::Camera, tag("Camera")),
        value(SceneObjectType::Sampler, tag("Sampler")),
        value(SceneObjectType::Integrator, tag("Integrator")),
        value(SceneObjectType::PixelFilter, tag("PixelFilter")),
        value(SceneObjectType::Film, tag("Film")),
    ))(input)
}

fn parse_world_object_type<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, WorldObjectType, E> {
    alt((
        value(WorldObjectType::LightSource, tag("LightSource")),
        value(WorldObjectType::AreaLightSource, tag("AreaLightSource")),
        value(WorldObjectType::Material, tag("Material")),
        value(WorldObjectType::MakeNamedMaterial, tag("MakeNamedMaterial")),
        value(WorldObjectType::Shape, tag("Shape")),
    ))(input)
}

fn parse_scene_object<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, SceneObject, E> {
    let (rest, ty) = parse_scene_object_type(input)?;
    let (rest, t) = preceded(sp, parse_str)(rest)?;
    let (rest, arguments) = preceded(sp, many0(preceded(sp, parse_argument)))(rest)?;

    Ok((
        rest,
        SceneObject {
            object_type: ty,
            t,
            arguments,
        },
    ))
}

fn parse_world_object<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, WorldObject, E> {
    let (rest, ty) = parse_world_object_type(input)?;
    let (rest, t) = preceded(sp, parse_str)(rest)?;
    let (rest, arguments) = preceded(sp, many0(preceded(sp, parse_argument)))(rest)?;

    Ok((
        rest,
        WorldObject {
            object_type: ty,
            t,
            arguments,
        },
    ))
}

fn parse_attribute_statement<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Vec<World>, E> {
    let (rest, _) = tag("AttributeBegin")(input)?;
    let (rest, worlds) = many0(preceded(sp, parse_world))(rest)?;
    let (rest, _) = preceded(sp, tag("AttributeEnd"))(rest)?;

    Ok((rest, worlds))
}

fn parse_transform_statement<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Vec<World>, E> {
    let (rest, _) = tag("TransformBegin")(input)?;
    let (rest, worlds) = many0(preceded(sp, parse_world))(rest)?;
    let (rest, _) = preceded(sp, tag("TransformEnd"))(rest)?;

    Ok((rest, worlds))
}

fn parse_transrate<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Vec3A, E> {
    let (rest, _) = tag("Translate")(input)?;
    preceded(sp, parse_vec3)(rest)
}

fn parse_scale<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Vec3A, E> {
    let (rest, _) = tag("Scale")(input)?;
    preceded(sp, parse_vec3)(rest)
}

fn parse_rotate<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, AxisAngle, E> {
    let (rest, _) = tag("Rotate")(input)?;
    preceded(sp, parse_axis_angle)(rest)
}

fn parse_texture<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Texture<'a>, E> {
    let (rest, _) = tag("Texture")(input)?;
    let (rest, name) = preceded(sp, parse_str)(rest)?;
    let (rest, value_type) = preceded(sp, parse_str)(rest)?;
    let (rest, t) = preceded(sp, parse_str)(rest)?;
    let (rest, arguments) = preceded(sp, many0(preceded(sp, parse_argument)))(rest)?;

    Ok((
        rest,
        Texture {
            name,
            value_type,
            obj: Object {
                object_type: (),
                t,
                arguments,
            },
        },
    ))
}

fn parse_world<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, World, E> {
    alt((
        map(parse_texture, World::Texture),
        map(parse_named_material, World::NamedMaterial),
        map(parse_world_object, World::WorldObject),
        map(parse_attribute_statement, World::Attribute),
        map(parse_transform_statement, World::TransformBeginEnd),
        map(parse_transform, World::Transform),
        map(parse_transrate, World::Translate),
        map(parse_scale, World::Scale),
        map(parse_rotate, World::Rotate),
    ))(input)
}

fn parse_world_statement<'a, E: ParseError<&'a str>>(
    input: &'a str,
) -> IResult<&'a str, Vec<World>, E> {
    let (rest, _) = tag("WorldBegin")(input)?;
    let (rest, worlds) = many0(preceded(sp, parse_world))(rest)?;
    let (rest, _) = preceded(sp, tag("WorldEnd"))(rest)?;

    Ok((rest, worlds))
}

fn parse_scene<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Scene, E> {
    alt((
        map(parse_look_at, Scene::LookAt),
        map(parse_rotate, Scene::Rotate),
        map(parse_transform, Scene::Transform),
        map(parse_scene_object, Scene::SceneObject),
        map(parse_world_statement, Scene::World),
    ))(input)
}

fn parse_all<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, Vec<Scene>, E> {
    // complete(terminated(parse_scene, eof))(input)
    let mut result = Vec::new();
    let mut rest = input;

    loop {
        if let Ok((rest, _)) = preceded(sp, eof::<_, Error<_>>)(rest) {
            return Ok((rest, result));
        }

        let (r, scene) = preceded(sp, parse_scene)(rest)?;
        result.push(scene);
        rest = r;
    }
}

pub fn parse_pbrt<'a, E: ParseError<&'a str>>(input: &'a str) -> Result<Vec<Scene>, E> {
    let (_rest, scene) = parse_all(input).finish()?;
    Ok(scene)
}

#[cfg(test)]
mod test {
    use nom::error::Error;

    use super::*;

    #[test]
    fn test_parse_space() {
        assert_eq!(space::<Error<&str>>("    "), Ok(("", "    ")));
    }

    #[test]
    fn test_parse_comment() {
        assert_eq!(comment::<Error<&str>>("#Hello"), Ok(("", "Hello")));
    }

    #[test]
    fn test_sp() {
        assert_eq!(sp::<Error<&str>>("    # aaaaa"), Ok(("", ())));
    }

    #[test]
    fn test_parse_integer() {
        assert_eq!(integer::<Error<&str>>("42"), Ok(("", 42)));
        assert_eq!(integer::<Error<&str>>("-42"), Ok(("", -42)));
    }

    #[test]
    fn test_parse_str() {
        assert_eq!(parse_str::<Error<&str>>(r#""aaa""#), Ok(("", "aaa")));

        assert_eq!(
            parse_str::<Error<&str>>(r#""geometry/room-teapot.pbrt""#),
            Ok(("", "geometry/room-teapot.pbrt"))
        );
    }

    #[test]
    fn test_parse_argument() {
        assert_eq!(
            parse_argument::<Error<&str>>(
                "\"point P\" [ -20 -20 0   20 -20 0   20 20 0   -20 20 0 ]"
            ),
            Ok((
                "",
                Argument {
                    name: "P",
                    value: Value::Point(vec![
                        vec3a(-20.0, -20.0, 0.0),
                        vec3a(20.0, -20.0, 0.0),
                        vec3a(20.0, 20.0, 0.0),
                        vec3a(-20.0, 20.0, 0.0)
                    ],)
                }
            ))
        )
    }

    #[test]
    fn test_parse_look_at() {
        assert_eq!(
            LookAt {
                eye: vec3a(3.0, 4.0, 1.5),
                look_at: vec3a(0.5, 0.5, 0.0),
                up: vec3a(0.0, 0.0, 1.0)
            },
            parse_look_at::<Error<&str>>(
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
                parse_scene_object::<Error<&str>>(q).unwrap()
            );
        }
    }

    #[test]
    fn test_parse_scene_object2() {
        assert_eq!(
            parse_scene_object::<Error<&str>>(r#"Integrator "path" "integer maxdepth" [ 65 ]"#),
            Ok((
                "",
                SceneObject {
                    object_type: SceneObjectType::Integrator,
                    t: "path",
                    arguments: vec![Argument {
                        name: "maxdepth",
                        value: Value::Integer(vec![65])
                    }]
                }
            ))
        );
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
            parse_world_object::<Error<&str>>(r#"LightSource "infinite" "rgb L" [.4 .45 .5]"#)
                .unwrap()
        );
    }

    #[test]
    fn test_parse_pbrt() {
        parse_pbrt::<Error<&str>>(
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

    #[test]
    fn test_parse_pbrt2() {
        parse_pbrt::<Error<&str>>(
            r#"
        WorldBegin

        AttributeBegin
        Material "matte" "rgb Kd" [0.1 0.2 0.1]
        Translate 0 0 -1
        Shape "trianglemesh"
            "integer indices" [0 1 2 0 2 3]
            "point P" [ -20 -20 0   20 -20 0   20 20 0   -20 20 0 ]
            "normal N" [ 0 0 1   0 0 1   0 0 1   0 0 1 ]
        AttributeEnd

        WorldEnd
        "#,
        )
        .unwrap();
    }
}
