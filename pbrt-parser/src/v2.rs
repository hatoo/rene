use chumsky::prelude::*;
use glam::{vec3a, vec4, Mat4, Vec3A, Vec4};

#[derive(Debug)]
pub enum Scene {
    Transform(Mat4),
    ConcatTransform(Mat4),
    LookAt(LookAt),
    Rotate(AxisAngle),
    Scale(Vec3A),
    Translate(Vec3A),
    SceneObject(SceneObject),
    World(Vec<World>),
}

#[derive(Clone, Debug)]
pub struct AxisAngle {
    pub axis: Vec3A,
    pub angle: f32,
}

#[derive(Clone, Debug)]
pub struct Texture {
    pub name: String,
    pub value_type: String,
    pub obj: Object<()>,
}

#[derive(Clone, Debug)]
pub enum World {
    WorldObject(WorldObject),
    Attribute(Vec<World>),
    TransformBeginEnd(Vec<World>),
    ObjectBeginEnd(String, Vec<World>),
    ObjectInstance(String),
    Transform(Mat4),
    ConcatTransform(Mat4),
    Translate(Vec3A),
    CoordSysTransform(String),
    Scale(Vec3A),
    Rotate(AxisAngle),
    Texture(Texture),
    NamedMaterial(String),
    MediumInterface(String, String),
    ReverseOrientation,
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
    Bool(Vec<bool>),
    Integer(Vec<i32>),
    Rgb(Vec<f32>),
    BlackBody(Vec<f32>),
    Point(Vec<Vec3A>),
    Normal(Vec<Vec3A>),
    String(Vec<String>),
    Texture(Vec<String>),
    Spectrum(String),
}

#[derive(PartialEq, Debug, Clone)]
pub struct Argument {
    pub name: String,
    pub value: Value,
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
    MakeNamedMedium,
    Shape,
}
#[derive(PartialEq, Debug, Clone)]
pub struct Object<T> {
    pub object_type: T,
    pub t: String,
    pub arguments: Vec<Argument>,
}

pub type SceneObject = Object<SceneObjectType>;
pub type WorldObject = Object<WorldObjectType>;

impl<T> Object<T> {
    pub fn get_value(&self, name: &str) -> Option<&Value> {
        self.arguments
            .iter()
            .find(|a| a.name == name)
            .map(|a| &a.value)
    }
}

fn comment() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    just('#')
        .then(take_until(text::newline().ignored().or(end())))
        .ignored()
        .labelled("comment")
}

fn sp() -> impl Parser<char, (), Error = Simple<char>> + Clone {
    comment()
        .padded()
        .repeated()
        .at_least(1)
        .ignored()
        .or(text::whitespace().ignored())
        .labelled("sp")
}

fn float() -> impl Parser<char, f32, Error = Simple<char>> + Clone {
    let frac = just('.').chain(text::digits(10));

    let exp = just('e')
        .or(just('E'))
        .chain(just('+').or(just('-')).or_not())
        .chain(text::digits(10));

    just('-')
        .or_not()
        .chain(text::int(10).chain(frac.or_not().flatten()).or(frac))
        .chain::<char, _, _>(exp.or_not().flatten())
        .collect::<String>()
        .from_str()
        .unwrapped()
        .labelled("float")
}

fn integer() -> impl Parser<char, i32, Error = Simple<char>> {
    just('-')
        .or_not()
        .chain::<char, _, _>(text::int(10))
        .collect::<String>()
        .from_str()
        .unwrapped()
        .labelled("integer")
}

fn string() -> impl Parser<char, String, Error = Simple<char>> {
    let escape = just('\\').ignore_then(
        just('\\')
            .or(just('/'))
            .or(just('"'))
            .or(just('b').to('\x08'))
            .or(just('f').to('\x0C'))
            .or(just('n').to('\n'))
            .or(just('r').to('\r'))
            .or(just('t').to('\t')),
    );

    filter(|c| *c != '\\' && *c != '"')
        .or(escape)
        .repeated()
        .delimited_by(just('"'), just('"'))
        .collect::<String>()
        .labelled("string")
}

fn bool() -> impl Parser<char, bool, Error = Simple<char>> {
    just("true")
        .to(true)
        .or(just("false").to(false))
        .labelled("bool")
}

fn parse_vec3() -> impl Parser<char, Vec3A, Error = Simple<char>> {
    let f = float().then_ignore(sp());
    f.clone()
        .then(f.clone())
        .then(f)
        .map(|((x, y), z)| vec3a(x, y, z))
        .labelled("vec3")
}

fn parse_vec4() -> impl Parser<char, Vec4, Error = Simple<char>> {
    let f = float().then_ignore(sp());
    f.clone()
        .then(f.clone())
        .then(f.clone())
        .then(f)
        .map(|(((x, y), z), w)| vec4(x, y, z, w))
        .labelled("vec4")
}

fn parse_transform() -> impl Parser<char, Mat4, Error = Simple<char>> {
    just("Transform")
        .then_ignore(sp())
        .ignore_then(
            parse_vec4()
                .then(parse_vec4())
                .then(parse_vec4())
                .then(parse_vec4())
                .delimited_by(just('[').then_ignore(sp()), just(']')),
        )
        .map(|(((x, y), z), w)| Mat4::from_cols(x, y, z, w))
        .labelled("Transform")
}

fn parse_concat_transform() -> impl Parser<char, Mat4, Error = Simple<char>> {
    just("ConcatTransform")
        .then_ignore(sp())
        .ignore_then(
            parse_vec4()
                .then(parse_vec4())
                .then(parse_vec4())
                .then(parse_vec4()),
        )
        .delimited_by(just('[').then_ignore(sp()), just(']'))
        .map(|(((x, y), z), w)| Mat4::from_cols(x, y, z, w))
        .labelled("ConcatTransform")
}

fn parse_look_at() -> impl Parser<char, LookAt, Error = Simple<char>> {
    just("LookAt")
        .then_ignore(sp())
        .ignore_then(parse_vec3().then_ignore(sp()))
        .then(parse_vec3().then_ignore(sp()))
        .then(parse_vec3().then_ignore(sp()))
        .map(|((eye, look_at), up)| LookAt { eye, look_at, up })
        .labelled("LookAt")
}

fn parse_rotate() -> impl Parser<char, AxisAngle, Error = Simple<char>> {
    just("Rotate")
        .then_ignore(sp())
        .ignore_then(float().then_ignore(sp()))
        .then(parse_vec3())
        .map(|(angle, axis)| AxisAngle { angle, axis })
        .labelled("Rotate")
}

fn parse_scale() -> impl Parser<char, Vec3A, Error = Simple<char>> {
    just("Scale")
        .then_ignore(sp())
        .ignore_then(parse_vec3())
        .labelled("Scale")
}

fn parse_translate() -> impl Parser<char, Vec3A, Error = Simple<char>> {
    just("Translate")
        .then_ignore(sp())
        .ignore_then(parse_vec3())
        .labelled("Translate")
}

fn bracket<T>(
    parser: impl Parser<char, T, Error = Simple<char>>,
) -> impl Parser<char, Vec<T>, Error = Simple<char>> {
    parser
        .then_ignore(sp())
        .repeated()
        .delimited_by(just('[').then_ignore(sp()), just(']'))
}

#[derive(Clone, Copy, Debug)]
enum ArgumentType {
    Float,
    Bool,
    Rgb,
    BlackBody,
    Integer,
    Point,
    Normal,
    String,
    Texture,
    Spectrum,
}

impl ArgumentType {
    fn parse(self) -> impl Parser<char, Value, Error = Simple<char>> {
        match self {
            Self::Float => float()
                .map(|f| vec![f])
                .or(bracket(float()))
                .map(Value::Float)
                .labelled("float")
                .boxed(),
            Self::Bool => bool()
                .map(|b| vec![b])
                .or(bracket(bool()))
                .map(Value::Bool)
                .labelled("bool")
                .boxed(),
            Self::Rgb => bracket(float()).map(Value::Rgb).labelled("rgb").boxed(),
            Self::BlackBody => bracket(float())
                .map(Value::BlackBody)
                .labelled("blackbody")
                .boxed(),
            Self::Integer => integer()
                .map(|i| vec![i])
                .or(bracket(integer()))
                .map(Value::Integer)
                .labelled("integer")
                .boxed(),
            Self::Point => bracket(float())
                .validate(|v, span, emit| {
                    if v.len() % 3 != 0 {
                        emit(Simple::custom(
                            span,
                            format!(
                                "length of point value must be multiple of 3. It was {}",
                                v.len(),
                            ),
                        ));
                    }
                    v
                })
                .map(|v| Value::Point(v.chunks(3).map(|p| vec3a(p[0], p[1], p[2])).collect()))
                .labelled("point")
                .boxed(),
            Self::Normal => bracket(float())
                .validate(|v, span, emit| {
                    if v.len() % 3 != 0 {
                        emit(Simple::custom(
                            span,
                            format!(
                                "length of normal value must be multiple of 3. It was {}",
                                v.len(),
                            ),
                        ));
                    }
                    v
                })
                .map(|v| Value::Normal(v.chunks(3).map(|p| vec3a(p[0], p[1], p[2])).collect()))
                .labelled("normal")
                .boxed(),
            Self::String => string()
                .map(|s| vec![s])
                .or(bracket(string()))
                .map(Value::String)
                .labelled("string")
                .boxed(),
            Self::Texture => string()
                .map(|s| vec![s])
                .or(bracket(string()))
                .map(Value::Texture)
                .labelled("texture")
                .boxed(),
            Self::Spectrum => string().map(Value::Spectrum).labelled("spectrum").boxed(),
        }
    }
}

fn parse_argument_type_name() -> impl Parser<char, (ArgumentType, String), Error = Simple<char>> {
    choice((
        just("float").to(ArgumentType::Float),
        just("bool").to(ArgumentType::Bool),
        just("integer").to(ArgumentType::Integer),
        just("string").to(ArgumentType::String),
        just("point").to(ArgumentType::Point),
        just("normal").to(ArgumentType::Normal),
        just("texture").to(ArgumentType::Texture),
        just("blackbody").to(ArgumentType::BlackBody),
        just("rgb").or(just("color")).to(ArgumentType::Rgb),
        just("spectrum").to(ArgumentType::Spectrum),
    ))
    .then_ignore(text::whitespace())
    .then(text::ident())
    .delimited_by(just('"'), just('"'))
    .labelled("Argument type and name")
}

fn parse_argument() -> impl Parser<char, Argument, Error = Simple<char>> {
    parse_argument_type_name()
        .then_ignore(sp())
        .then_with(|(ty, name)| {
            ty.parse().map(move |value| Argument {
                // TODO: Can we remove this clone?
                name: name.clone(),
                value,
            })
        })
        .labelled("argument")
}

fn parse_scene_object() -> impl Parser<char, SceneObject, Error = Simple<char>> {
    choice((
        just("Camera").to(SceneObjectType::Camera),
        just("Sampler").to(SceneObjectType::Sampler),
        just("Integrator").to(SceneObjectType::Integrator),
        just("PixelFilter").to(SceneObjectType::PixelFilter),
        just("Film").to(SceneObjectType::Film),
    ))
    .then_ignore(sp())
    .then(string())
    .then_ignore(sp())
    .then(parse_argument().then_ignore(sp()).repeated())
    .map(|((object_type, t), arguments)| SceneObject {
        object_type,
        t,
        arguments,
    })
    .labelled("scene object")
}

fn parse_world_statement() -> impl Parser<char, Vec<World>, Error = Simple<char>> {
    parse_worlds().delimited_by(just("WorldBegin").then_ignore(sp()), just("WorldEnd"))
}

fn parse_scene() -> impl Parser<char, Scene, Error = Simple<char>> {
    choice((
        parse_look_at().map(Scene::LookAt),
        parse_rotate().map(Scene::Rotate),
        parse_scale().map(Scene::Scale),
        parse_translate().map(Scene::Translate),
        parse_concat_transform().map(Scene::ConcatTransform),
        parse_transform().map(Scene::Transform),
        parse_scene_object().map(Scene::SceneObject),
        parse_world_statement().map(Scene::World),
    ))
    .labelled("scene")
}

pub fn parse_pbrt() -> impl Parser<char, Vec<Scene>, Error = Simple<char>> {
    parse_scene()
        .then_ignore(sp())
        .repeated()
        .padded_by(sp())
        .then_ignore(end())
}

// World stuff
fn parse_texture() -> impl Parser<char, Texture, Error = Simple<char>> {
    just("Texture")
        .then_ignore(sp())
        .ignore_then(
            string()
                .then_ignore(sp())
                .then(string().then_ignore(sp()))
                .then(string().then_ignore(sp()))
                .then(parse_argument().then_ignore(sp()).repeated()),
        )
        .map(|(((name, value_type), t), arguments)| Texture {
            name,
            value_type,
            obj: Object {
                object_type: (),
                t,
                arguments,
            },
        })
        .labelled("texture")
}

fn parse_named_material() -> impl Parser<char, String, Error = Simple<char>> {
    just("NamedMaterial")
        .then_ignore(sp())
        .ignore_then(string())
        .labelled("named material")
}

fn parse_world_object() -> impl Parser<char, WorldObject, Error = Simple<char>> {
    choice((
        just("LightSource").to(WorldObjectType::LightSource),
        just("AreaLightSource").to(WorldObjectType::AreaLightSource),
        just("Material").to(WorldObjectType::Material),
        just("MakeNamedMaterial").to(WorldObjectType::MakeNamedMaterial),
        just("MakeNamedMedium").to(WorldObjectType::MakeNamedMedium),
        just("Shape").to(WorldObjectType::Shape),
    ))
    .then_ignore(sp())
    .then(string())
    .then_ignore(sp())
    .then(parse_argument().then_ignore(sp()).repeated())
    .map(|((object_type, t), arguments)| WorldObject {
        object_type,
        t,
        arguments,
    })
    .labelled("world object")
}

fn parse_object_instance() -> impl Parser<char, String, Error = Simple<char>> {
    just("ObjectInstance")
        .then_ignore(sp())
        .ignore_then(string())
        .labelled("object instance")
}

fn parse_coord_sys_transform() -> impl Parser<char, String, Error = Simple<char>> {
    just("CoordSysTransform")
        .then_ignore(sp())
        .ignore_then(string())
        .labelled("CoordSysTransform")
}

fn parse_medium_interface() -> impl Parser<char, (String, String), Error = Simple<char>> {
    just("MediumInterface")
        .then_ignore(sp())
        .ignore_then(string())
        .then_ignore(sp())
        .then(string())
        .labelled("MediumInterface")
}

fn parse_worlds() -> impl Parser<char, Vec<World>, Error = Simple<char>> {
    recursive(|bf| {
        choice((
            parse_texture().map(World::Texture),
            parse_named_material().map(World::NamedMaterial),
            parse_world_object().map(World::WorldObject),
            parse_object_instance().map(World::ObjectInstance),
            parse_transform().map(World::Transform),
            parse_concat_transform().map(World::ConcatTransform),
            parse_translate().map(World::Translate),
            parse_scale().map(World::Scale),
            parse_rotate().map(World::Rotate),
            parse_coord_sys_transform().map(World::CoordSysTransform),
            parse_medium_interface().map(|(i, e)| World::MediumInterface(i, e)),
            just("ReverseOrientation").to(World::ReverseOrientation),
            bf.clone()
                .delimited_by(
                    just("AttributeBegin").then_ignore(sp()),
                    just("AttributeEnd"),
                )
                .map(World::Attribute),
            bf.clone()
                .delimited_by(
                    just("TransformBegin").then_ignore(sp()),
                    just("TransformEnd"),
                )
                .map(World::Attribute),
            just("OnjectBegin")
                .then_ignore(sp())
                .ignore_then(string().then_ignore(sp()).then(bf))
                .map(|(name, worlds)| World::ObjectBeginEnd(name, worlds)),
        ))
        .then_ignore(sp())
        .repeated()
    })
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_comment() {
        comment().parse("# Hello").unwrap();
    }

    #[test]
    fn test_sp() {
        sp().parse("# Hello\n   \n").unwrap();
        sp().parse(
            r#"# hello
        # world"#,
        )
        .unwrap();
        sp().parse("\n   \n").unwrap();
        sp().parse(" ").unwrap();
        sp().parse("").unwrap();
    }

    #[test]
    fn test_float() {
        assert_eq!(float().parse("1").unwrap(), 1.0);
        assert_eq!(float().parse("2.25").unwrap(), 2.25);
        assert_eq!(float().parse("1e5").unwrap(), 1e5);
        assert_eq!(float().parse("1e-5").unwrap(), 1e-5);
        assert_eq!(float().parse(".9").unwrap(), 0.9);
    }

    #[test]
    fn test_integer() {
        assert_eq!(integer().parse("1").unwrap(), 1);
        assert_eq!(integer().parse("114514").unwrap(), 114514);
        assert_eq!(integer().parse("-200").unwrap(), -200);
    }

    #[test]
    fn test_string() {
        assert_eq!(string().parse(r#""TEST""#).unwrap(), "TEST");
    }

    #[test]
    fn test_parse_vec4() {
        assert_eq!(
            parse_vec4()
                .parse(
                    r#"1 # this is 1 
                    # aaa
                    2 # this is 2
                    3
                    4"#
                )
                .unwrap(),
            vec4(1.0, 2.0, 3.0, 4.0)
        );
    }

    #[test]
    fn test_parse_argument() {
        assert_eq!(
            parse_argument().parse(r#""string test" "OK""#).unwrap(),
            Argument {
                name: "test".to_string(),
                value: Value::String(vec!["OK".to_string()])
            }
        );
        assert_eq!(
            parse_argument().parse(r#""float test" [1 2 3]"#).unwrap(),
            Argument {
                name: "test".to_string(),
                value: Value::Float(vec![1.0, 2.0, 3.0])
            }
        );

        assert_eq!(
            parse_argument().parse(r#""rgb Kd" [ .7 .2 .2 ]"#).unwrap(),
            Argument {
                name: "Kd".to_string(),
                value: Value::Rgb(vec![0.7, 0.2, 0.2])
            }
        );
    }

    #[test]
    fn test_world() {
        let src = r#"LightSource "infinite" "rgb L" [.4 .45 .5]"#;

        parse_worlds().parse(src).unwrap();
    }

    #[test]
    fn test_world_statement() {
        let src = r#"WorldBegin
# uniform blue-ish illumination from all directions
LightSource "infinite" "rgb L" [.4 .45 .5]

AttributeBegin
  Material "matte" "rgb Kd" [ .7 .2 .2 ]
  Shape "sphere" "float radius" 1
AttributeEnd

WorldEnd
        "#;

        parse_world_statement().parse(src).unwrap();
    }

    #[test]
    fn test_sphere() {
        let src = r#"
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
        "#;

        parse_pbrt().parse(src).unwrap();
    }
}
