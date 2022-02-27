use std::{borrow::Cow, fs::File, io::Read, path::Path};

use nom::{
    branch::alt,
    bytes::complete::{escaped, tag, take_while, take_while1},
    character::complete::{char, none_of, one_of},
    combinator::{cut, value},
    error::{Error, ParseError},
    multi::many0,
    sequence::{preceded, terminated},
    IResult,
};

fn comment<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    let (rest, _) = char('#')(input)?;
    take_while(|c| c != '\n')(rest)
}

fn space<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, &'a str, E> {
    let chars = " \t\r\n";

    take_while1(move |c| chars.contains(c))(input)
}

fn sp<'a, E: ParseError<&'a str>>(input: &'a str) -> IResult<&'a str, (), E> {
    value((), many0(alt((space, comment))))(input)
}

pub fn parse_str<'a, E: ParseError<&'a str>>(i: &'a str) -> IResult<&'a str, &'a str, E> {
    fn parse<'a, E: ParseError<&'a str>>(i: &'a str) -> IResult<&'a str, &'a str, E> {
        alt((escaped(none_of("\""), '\\', one_of("\"n\\")), tag("")))(i)
    }
    preceded(char('\"'), cut(terminated(parse, char('\"'))))(i)
}

pub fn expand_include<P: AsRef<Path>>(
    input: &str,
    current_dir: P,
) -> Result<Cow<str>, std::io::Error> {
    let mut expanded = false;
    let mut result = String::new();
    let mut rest = input;

    loop {
        if let Some(mid) = rest.find("Include") {
            let (head, r) = rest.split_at(mid);

            result += head;

            match preceded(preceded(tag("Include"), sp), parse_str::<Error<_>>)(r) {
                Ok((r, path)) => {
                    let mut buf = String::new();

                    let mut current_path = current_dir.as_ref().to_owned();
                    current_path.push(path);

                    File::open(&current_path)?.read_to_string(&mut buf)?;

                    match expand_include(&buf, current_dir.as_ref())? {
                        Cow::Borrowed(_) => {}
                        Cow::Owned(s) => buf = s,
                    }

                    result += &buf;
                    expanded = true;

                    rest = r;
                }
                Err(_) => {
                    let (r, _) = tag::<_, _, Error<_>>("Include")(r).unwrap();
                    result += "Include";
                    rest = r;
                }
            }
        } else {
            return Ok(if expanded {
                result += rest;
                Cow::Owned(result)
            } else {
                Cow::Borrowed(input)
            });
        }
    }
}
