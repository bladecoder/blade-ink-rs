pub fn parse_header(line: &str) -> Option<Header> {
    let trimmed = line.trim();

    if trimmed.starts_with("===") || trimmed.starts_with("==") {
        let inner = trimmed.trim_matches('=').trim();
        if let Some(rest) = inner.strip_prefix("function") {
            let (name, parameters, ref_parameters, divert_parameters) =
                parse_header_signature(rest.trim())?;
            return Some(Header::Function {
                name,
                parameters,
                ref_parameters,
                divert_parameters,
            });
        }

        let (name, parameters, ref_parameters, divert_parameters) = parse_header_signature(inner)?;
        return Some(Header::Knot {
            name,
            parameters,
            ref_parameters,
            divert_parameters,
        });
    }

    if trimmed.starts_with('=') {
        let inner = trimmed.trim_start_matches('=').trim();
        let (name, parameters, ref_parameters, divert_parameters) = parse_header_signature(inner)?;
        return Some(Header::Stitch {
            name,
            parameters,
            ref_parameters,
            divert_parameters,
        });
    }

    None
}

/// Returns (name, params, ref_params, divert_params)
type HeaderSignature = (String, Vec<String>, Vec<String>, Vec<String>);

fn parse_header_signature(text: &str) -> Option<HeaderSignature> {
    use expression::split_top_level_commas;

    let open = text.find('(');
    let close = text.rfind(')');

    match (open, close) {
        (Some(open), Some(close)) if close > open => {
            let name = parse_path_identifier(text[..open].trim())?.to_owned();
            let mut parameters = Vec::new();
            let mut ref_parameters = Vec::new();
            let mut divert_parameters = Vec::new();
            for part in split_top_level_commas(&text[open + 1..close]) {
                let trimmed = part.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Strip divert-type annotation: "-> paramName" → "paramName"
                let parameter = if let Some(name) = trimmed.strip_prefix("->").map(str::trim) {
                    let name = name.to_owned();
                    divert_parameters.push(name.clone());
                    name
                } else if let Some(name) = trimmed.strip_prefix("ref ") {
                    let name = name.trim().to_owned();
                    ref_parameters.push(name.clone());
                    name
                } else {
                    trimmed.to_owned()
                };
                parameters.push(parameter);
            }
            Some((name, parameters, ref_parameters, divert_parameters))
        }
        _ => Some((
            parse_path_identifier(text)?.to_owned(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )),
    }
}
