//! Structured types in `.pxi` v2 (JSON, no external deps).

/// Enum variant payload in a `.pxi` export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PxiVariantPayload {
    /// Unit variant.
    Unit,
    /// Tuple variant (`payload` array of types).
    Tuple(Vec<PxiType>),
    /// Struct variant (`fields` array).
    Struct(Vec<PxiField>),
}

/// One struct field or struct-variant field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PxiField {
    /// Field name.
    pub name: String,
    /// Field type.
    pub ty: PxiType,
}

/// One enum variant in a `.pxi` export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PxiVariant {
    /// Variant name.
    pub name: String,
    /// Discriminant tag.
    pub tag: u32,
    /// Payload shape.
    pub payload: PxiVariantPayload,
}

/// Structured type tree for `.pxi` v2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PxiType {
    /// Primitive (`s32`, `bool`, …).
    Primitive(String),
    /// Unit `()`.
    Unit,
    /// Named type (`logical::Path` + generic args).
    Named {
        /// Stable path (`module::Name`).
        path: String,
        /// Generic arguments.
        args: Vec<PxiType>,
    },
    /// Tuple.
    Tuple(Vec<PxiType>),
    /// Fixed array `[T; N]`.
    Array {
        /// Element type.
        elem: Box<PxiType>,
        /// Length.
        len: u32,
    },
    /// Slice `[T]`.
    Slice(Box<PxiType>),
    /// Reference `&T` / `&mut T`.
    Ref {
        /// `true` for `&mut`.
        mut_: bool,
        /// Inner type.
        inner: Box<PxiType>,
    },
    /// Raw pointer.
    Ptr {
        /// `true` for `*mut`.
        mut_: bool,
        /// Inner type.
        inner: Box<PxiType>,
    },
    /// Function type.
    Fn {
        /// Parameter types.
        params: Vec<PxiType>,
        /// Return type.
        ret: Box<PxiType>,
    },
    /// Struct type with fields.
    Struct {
        /// Fields in declaration order.
        fields: Vec<PxiField>,
    },
    /// Enum type with variants.
    Enum {
        /// Variants.
        variants: Vec<PxiVariant>,
    },
    /// Type alias target.
    Alias(Box<PxiType>),
}

impl PxiType {
    /// Serializes this type as a JSON object fragment (no surrounding braces).
    #[must_use]
    pub fn to_json(&self) -> String {
        match self {
            Self::Primitive(name) => {
                format!(
                    "{{ \"kind\": \"primitive\", \"name\": {} }}",
                    json_string(name)
                )
            }
            Self::Unit => "{ \"kind\": \"unit\" }".to_owned(),
            Self::Named { path, args } => {
                let args_json: Vec<_> = args.iter().map(PxiType::to_json).collect();
                format!(
                    "{{ \"kind\": \"named\", \"path\": {}, \"args\": [{}] }}",
                    json_string(path),
                    args_json.join(", ")
                )
            }
            Self::Tuple(elems) => {
                let inner: Vec<_> = elems.iter().map(PxiType::to_json).collect();
                format!(
                    "{{ \"kind\": \"tuple\", \"elems\": [{}] }}",
                    inner.join(", ")
                )
            }
            Self::Array { elem, len } => format!(
                "{{ \"kind\": \"array\", \"len\": {len}, \"elem\": {} }}",
                elem.to_json()
            ),
            Self::Slice(elem) => format!("{{ \"kind\": \"slice\", \"elem\": {} }}", elem.to_json()),
            Self::Ref { mut_, inner } => format!(
                "{{ \"kind\": \"ref\", \"mut\": {}, \"inner\": {} }}",
                if *mut_ { "true" } else { "false" },
                inner.to_json()
            ),
            Self::Ptr { mut_, inner } => format!(
                "{{ \"kind\": \"ptr\", \"mut\": {}, \"inner\": {} }}",
                if *mut_ { "true" } else { "false" },
                inner.to_json()
            ),
            Self::Fn { params, ret } => {
                let ps: Vec<_> = params.iter().map(PxiType::to_json).collect();
                format!(
                    "{{ \"kind\": \"fn\", \"params\": [{}], \"ret\": {} }}",
                    ps.join(", "),
                    ret.to_json()
                )
            }
            Self::Struct { fields } => {
                let fs: Vec<_> = fields
                    .iter()
                    .map(|f| {
                        format!(
                            "{{ \"name\": {}, \"type\": {} }}",
                            json_string(&f.name),
                            f.ty.to_json()
                        )
                    })
                    .collect();
                format!(
                    "{{ \"kind\": \"struct\", \"fields\": [{}] }}",
                    fs.join(", ")
                )
            }
            Self::Enum { variants } => {
                let vs: Vec<_> = variants.iter().map(PxiVariant::to_json).collect();
                format!(
                    "{{ \"kind\": \"enum\", \"variants\": [{}] }}",
                    vs.join(", ")
                )
            }
            Self::Alias(inner) => {
                format!("{{ \"kind\": \"alias\", \"inner\": {} }}", inner.to_json())
            }
        }
    }
}

impl PxiVariant {
    fn to_json(&self) -> String {
        let payload = match &self.payload {
            PxiVariantPayload::Unit => "null".to_owned(),
            PxiVariantPayload::Tuple(ts) => {
                let inner: Vec<_> = ts.iter().map(PxiType::to_json).collect();
                format!("[{}]", inner.join(", "))
            }
            PxiVariantPayload::Struct(fields) => {
                let fs: Vec<_> = fields
                    .iter()
                    .map(|f| {
                        format!(
                            "{{ \"name\": {}, \"type\": {} }}",
                            json_string(&f.name),
                            f.ty.to_json()
                        )
                    })
                    .collect();
                format!("{{ \"fields\": [{}] }}", fs.join(", "))
            }
        };
        format!(
            "{{ \"name\": {}, \"tag\": {}, \"payload\": {payload} }}",
            json_string(&self.name),
            self.tag
        )
    }
}

/// Parses a `"type": { ... }` object starting at `text` (after `"type":`).
///
/// # Errors
///
/// Returns `None` when the fragment is not a recognized type object.
#[must_use]
pub fn parse_type_value(text: &str) -> Option<PxiType> {
    let rest = text.trim_start();
    if !rest.starts_with('{') {
        return None;
    }
    parse_type_object(rest)
}

fn parse_type_object(s: &str) -> Option<PxiType> {
    let kind = extract_string_field(s, "kind")?;
    match kind.as_str() {
        "primitive" => {
            let name = extract_string_field(s, "name")?;
            Some(PxiType::Primitive(name))
        }
        "unit" => Some(PxiType::Unit),
        "named" => {
            let path = extract_string_field(s, "path")?;
            let args = parse_type_array(s, "args");
            Some(PxiType::Named { path, args })
        }
        "tuple" => {
            let elems = parse_type_array(s, "elems");
            Some(PxiType::Tuple(elems))
        }
        "array" => {
            let len = extract_u32_field(s, "len")?;
            let elem = parse_type_field(s, "elem")?;
            Some(PxiType::Array {
                elem: Box::new(elem),
                len,
            })
        }
        "slice" => {
            let elem = parse_type_field(s, "elem")?;
            Some(PxiType::Slice(Box::new(elem)))
        }
        "ref" => {
            let mut_ = extract_bool_field(s, "mut")?;
            let inner = parse_type_field(s, "inner")?;
            Some(PxiType::Ref {
                mut_,
                inner: Box::new(inner),
            })
        }
        "ptr" => {
            let mut_ = extract_bool_field(s, "mut")?;
            let inner = parse_type_field(s, "inner")?;
            Some(PxiType::Ptr {
                mut_,
                inner: Box::new(inner),
            })
        }
        "fn" => {
            let params = parse_type_array(s, "params");
            let ret = parse_type_field(s, "ret")?;
            Some(PxiType::Fn {
                params,
                ret: Box::new(ret),
            })
        }
        "struct" => {
            let fields = parse_fields_array(s, "fields")?;
            Some(PxiType::Struct { fields })
        }
        "enum" => {
            let variants = parse_variants_array(s)?;
            Some(PxiType::Enum { variants })
        }
        "alias" => {
            let inner = parse_type_field(s, "inner")?;
            Some(PxiType::Alias(Box::new(inner)))
        }
        _ => None,
    }
}

fn parse_type_array(s: &str, key: &str) -> Vec<PxiType> {
    let Some(slice) = array_slice_after_key(s, key) else {
        return Vec::new();
    };
    split_top_level_objects(slice)
        .iter()
        .filter_map(|obj| parse_type_object(obj))
        .collect()
}

/// Returns the interior of the `[...]` array after `"key":`, excluding brackets.
fn array_slice_after_key<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let pat = format!("\"{key}\":");
    let pos = s.find(&pat)? + pat.len();
    let rest = s[pos..].trim_start();
    if !rest.starts_with('[') {
        return None;
    }
    let mut depth = 0i32;
    let mut start_idx = None;
    for (i, ch) in rest.char_indices() {
        match ch {
            '[' => {
                if depth == 0 {
                    start_idx = Some(i + 1);
                }
                depth += 1;
            }
            ']' => {
                depth -= 1;
                if depth == 0 {
                    let st = start_idx?;
                    return Some(&rest[st..i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_type_field(s: &str, key: &str) -> Option<PxiType> {
    let pat = format!("\"{key}\":");
    let pos = s.find(&pat)? + pat.len();
    let rest = s[pos..].trim_start();
    parse_type_object(rest)
}

fn parse_fields_array(s: &str, key: &str) -> Option<Vec<PxiField>> {
    let slice = array_slice_after_key(s, key)?;
    let mut out = Vec::new();
    for obj in split_top_level_objects(slice) {
        let name = extract_string_field(&obj, "name")?;
        let ty = parse_type_field(&obj, "type")?;
        out.push(PxiField { name, ty });
    }
    Some(out)
}

fn parse_variants_array(s: &str) -> Option<Vec<PxiVariant>> {
    let slice = array_slice_after_key(s, "variants")?;
    let mut out = Vec::new();
    for obj in split_top_level_objects(slice) {
        let name = extract_string_field(&obj, "name")?;
        let tag = extract_u32_field(&obj, "tag")?;
        let payload = parse_variant_payload(&obj)?;
        out.push(PxiVariant { name, tag, payload });
    }
    Some(out)
}

fn parse_variant_payload(s: &str) -> Option<PxiVariantPayload> {
    let pat = "\"payload\":";
    let pos = s.find(pat)? + pat.len();
    let rest = s[pos..].trim_start();
    if rest.starts_with("null") {
        return Some(PxiVariantPayload::Unit);
    }
    if rest.starts_with('[') {
        let types = split_top_level_objects(rest)
            .iter()
            .filter_map(|obj| parse_type_object(obj))
            .collect();
        return Some(PxiVariantPayload::Tuple(types));
    }
    if rest.starts_with('{') {
        let fields = parse_fields_array(s, "fields")?;
        return Some(PxiVariantPayload::Struct(fields));
    }
    None
}

fn split_top_level_objects(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = None;
    for (i, ch) in s.char_indices() {
        match ch {
            '{' if depth == 0 => {
                start = Some(i);
                depth = 1;
            }
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(st) = start {
                        out.push(s[st..=i].to_owned());
                    }
                    start = None;
                }
            }
            '[' if depth == 0 && start.is_none() => {}
            _ => {}
        }
    }
    out
}

fn extract_string_field(s: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\":");
    let pos = s.find(&pat)? + pat.len();
    let rest = s[pos..].trim_start();
    if !rest.starts_with('"') {
        return None;
    }
    parse_json_string(&rest[1..])
}

fn extract_u32_field(s: &str, key: &str) -> Option<u32> {
    let pat = format!("\"{key}\":");
    let pos = s.find(&pat)? + pat.len();
    let rest = s[pos..].trim_start();
    let num: String = rest.chars().take_while(char::is_ascii_digit).collect();
    num.parse().ok()
}

fn extract_bool_field(s: &str, key: &str) -> Option<bool> {
    let pat = format!("\"{key}\":");
    let pos = s.find(&pat)? + pat.len();
    let rest = s[pos..].trim_start();
    if rest.starts_with("true") {
        Some(true)
    } else if rest.starts_with("false") {
        Some(false)
    } else {
        None
    }
}

fn parse_json_string(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '"' {
            return Some(out);
        }
        if c == '\\' {
            let next = chars.next()?;
            out.push(match next {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                other => other,
            });
        } else {
            out.push(c);
        }
    }
    None
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn pxi_type_round_trip_fn() {
        let ty = PxiType::Fn {
            params: vec![
                PxiType::Primitive("s32".to_owned()),
                PxiType::Primitive("s32".to_owned()),
            ],
            ret: Box::new(PxiType::Primitive("s32".to_owned())),
        };
        let json = ty.to_json();
        let back = parse_type_value(&json).expect("parse");
        assert_eq!(back, ty);
    }
}
