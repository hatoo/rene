use std::{borrow::Cow, fs::File, io::Read, path::Path};

use nom::{bytes::complete::tag, error::Error, sequence::preceded};

use crate::{parse_str, sp};

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

                    // TODO Error handling
                    File::open(&current_path)?.read_to_string(&mut buf)?;

                    current_path.pop();
                    match expand_include(&buf, &current_path)? {
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
