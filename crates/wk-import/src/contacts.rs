//! Address books exported from Google Contacts (Google CSV, Outlook CSV or
//! vCard) and matching their phone numbers to WhatsApp's.
//!
//! The msgstore has no names (they live in `wa.db`, which is not in the
//! backup); an address book export fills them in. Pure: no I/O.

/// A person in the address book: a display name and the raw numbers as typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Person {
    pub name: String,
    pub phones: Vec<String>,
}

/// Reads a Google Contacts export: vCard (`BEGIN:VCARD`) or CSV with a header
/// row. People without a name or without a phone number are left out.
pub fn parse(bytes: &[u8]) -> Vec<Person> {
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim_start_matches('\u{feff}');
    let people = if text.trim_start().to_ascii_uppercase().starts_with("BEGIN:VCARD") { parse_vcard(text) } else { parse_csv(text) };
    people.into_iter().filter(|p| !p.name.is_empty() && !p.phones.is_empty()).collect()
}

// ---------------------------------------------------------------- CSV

/// RFC 4180 rows: quoted fields may hold commas, quotes (`""`) and line breaks.
fn csv_rows(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => row.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
            }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !row.is_empty() {
        row.push(field);
        rows.push(row);
    }
    rows.retain(|r| r.iter().any(|f| !f.trim().is_empty()));
    rows
}

fn parse_csv(text: &str) -> Vec<Person> {
    let mut rows = csv_rows(text).into_iter();
    let Some(header) = rows.next() else { return Vec::new() };
    let col = |name: &str| header.iter().position(|h| h.trim().eq_ignore_ascii_case(name));
    let full = col("Name");
    let parts: Vec<usize> = [
        ["First Name", "Given Name"],
        ["Middle Name", "Additional Name"],
        ["Last Name", "Family Name"],
    ]
    .iter()
    .filter_map(|alts| alts.iter().find_map(|a| col(a)))
    .collect();
    let fallbacks: Vec<usize> = ["File As", "Nickname", "Organization Name", "Organization 1 - Name", "Company"]
        .iter()
        .filter_map(|a| col(a))
        .collect();
    // Google: "Phone 1 - Value"; Outlook: "Mobile Phone", "Home Phone"...
    let phone_cols: Vec<usize> = header
        .iter()
        .enumerate()
        .filter(|(_, h)| {
            let h = h.trim().to_ascii_lowercase();
            h.contains("phone") && !h.ends_with("type") && !h.ends_with("label")
        })
        .map(|(i, _)| i)
        .collect();

    rows.map(|r| {
        let get = |i: usize| r.get(i).map(|v| v.trim()).unwrap_or("");
        let joined = parts.iter().map(|&i| get(i)).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ");
        let name = full
            .map(get)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| (!joined.is_empty()).then_some(joined))
            .or_else(|| fallbacks.iter().map(|&i| get(i)).find(|s| !s.is_empty()).map(str::to_owned))
            .unwrap_or_default();
        let phones = phone_cols
            .iter()
            .flat_map(|&i| get(i).split(":::").map(|p| p.trim().to_owned()).collect::<Vec<_>>())
            .filter(|p| !p.is_empty())
            .collect();
        Person { name, phones }
    })
    .collect()
}

// ---------------------------------------------------------------- vCard

fn vcard_unescape(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    let mut chars = v.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n' | 'N') => out.push(' '),
                Some(other) => out.push(other),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out.trim().to_owned()
}

fn parse_vcard(text: &str) -> Vec<Person> {
    // Unfold: a line starting with a space or tab continues the previous one.
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        match (raw.strip_prefix(' ').or_else(|| raw.strip_prefix('\t')), lines.last_mut()) {
            (Some(rest), Some(last)) => last.push_str(rest),
            _ => lines.push(raw.to_owned()),
        }
    }
    let mut people = Vec::new();
    let mut fn_name = String::new();
    let mut n_name = String::new();
    let mut phones = Vec::new();
    for line in lines {
        let Some((prop, value)) = line.split_once(':') else { continue };
        // `item1.TEL;TYPE=CELL` → `TEL`
        let prop = prop.split(';').next().unwrap_or("");
        let prop = prop.rsplit('.').next().unwrap_or("").to_ascii_uppercase();
        match prop.as_str() {
            "BEGIN" => {
                fn_name.clear();
                n_name.clear();
                phones.clear();
            }
            "FN" => fn_name = vcard_unescape(value),
            "N" => {
                // family;given;additional;prefix;suffix
                let parts: Vec<String> = value.split(';').map(vcard_unescape).collect();
                let order = [3, 1, 2, 0, 4];
                n_name = order.iter().filter_map(|&i| parts.get(i)).filter(|s| !s.is_empty()).cloned().collect::<Vec<_>>().join(" ");
            }
            "TEL" => {
                let v = vcard_unescape(value);
                let v = v.strip_prefix("tel:").unwrap_or(&v).to_owned();
                if !v.is_empty() {
                    phones.push(v);
                }
            }
            "END" => {
                let name = if fn_name.is_empty() { n_name.clone() } else { fn_name.clone() };
                people.push(Person { name, phones: std::mem::take(&mut phones) });
            }
            _ => {}
        }
    }
    people
}

// ---------------------------------------------------------------- numbers

/// A number as WhatsApp writes it: country code and number, digits only
/// (`5581999410815`). `default_cc` is used for numbers saved without one
/// (`(81) 99941-0815`). `None` for something that is not a phone number.
pub fn normalize(raw: &str, default_cc: Option<&str>) -> Option<String> {
    let raw = raw.trim();
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() {
        return None;
    }
    let out = if raw.starts_with('+') {
        digits
    } else if let Some(rest) = digits.strip_prefix("00") {
        rest.to_owned()
    } else if let Some(cc) = default_cc {
        // Brazil: trunk prefix 0, optionally followed by a 2-digit carrier code
        // (`0 15 81 99941-0815`).
        let mut n = digits.trim_start_matches('0').to_owned();
        let international = |n: &str| match cc {
            "55" => n.starts_with("55") && (n.len() == 12 || n.len() == 13),
            _ => n.starts_with(cc) && n.len() >= 11,
        };
        if cc == "55" && digits.starts_with('0') && (n.len() == 12 || n.len() == 13) && !international(&n) {
            n = n[2..].to_owned();
        }
        if international(&n) {
            n
        } else if n.len() <= 11 {
            format!("{cc}{n}")
        } else {
            n
        }
    } else {
        digits
    };
    (8..=15).contains(&out.len()).then_some(out)
}

/// The forms a number may take on WhatsApp: itself and, in Brazil, with or
/// without the mobile "9" added in 2012–2016 (`55 81 9941-0815` ↔
/// `55 81 9 9941-0815`). Accounts registered before the change kept the old
/// form in WhatsApp, while address books usually have the new one.
pub fn variants(phone: &str) -> Vec<String> {
    let mut out = vec![phone.to_owned()];
    if let Some(rest) = phone.strip_prefix("55") {
        let (ddd, number) = rest.split_at(rest.len().min(2));
        if number.len() == 9 && number.starts_with('9') {
            out.push(format!("55{ddd}{}", &number[1..]));
        } else if number.len() == 8 && matches!(number.as_bytes()[0], b'6'..=b'9') {
            out.push(format!("55{ddd}9{number}"));
        }
    }
    out
}

/// The country code most of the history's numbers share, to complete address
/// book numbers saved without one. `None` when there is no clear majority.
pub fn infer_country_code<'a>(phones: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let phones: Vec<&str> = phones.into_iter().filter(|p| p.len() >= 10).collect();
    if phones.is_empty() {
        return None;
    }
    for len in [2, 1, 3] {
        let mut counts: std::collections::HashMap<&str, usize> = Default::default();
        for p in &phones {
            *counts.entry(&p[..len]).or_default() += 1;
        }
        if let Some((cc, n)) = counts.into_iter().max_by_key(|(_, n)| *n)
            && n * 2 > phones.len()
            && !cc.starts_with('0')
        {
            return Some(cc.to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(people: &[Person]) -> Vec<(&str, Vec<&str>)> {
        people.iter().map(|p| (p.name.as_str(), p.phones.iter().map(String::as_str).collect())).collect()
    }

    #[test]
    fn google_csv_current_format() {
        let csv = "\u{feff}First Name,Middle Name,Last Name,Nickname,Organization Name,Phone 1 - Label,Phone 1 - Value,Phone 2 - Label,Phone 2 - Value\n\
                   Ana,,Souza,,,Mobile,+55 81 99941-0815 ::: (81) 3333-4444,Work,\n\
                   ,,,,Padaria Pão Quente,Work,(81) 3222-1111,,\n\
                   Sem Telefone,,,,,,,,\n\
                   \"Bruno, o \"\"Grande\"\"\",,,,,Mobile,\"+1 202 555 0142\",,\n";
        assert_eq!(
            names(&parse(csv.as_bytes())),
            [
                ("Ana Souza", vec!["+55 81 99941-0815", "(81) 3333-4444"]),
                ("Padaria Pão Quente", vec!["(81) 3222-1111"]),
                ("Bruno, o \"Grande\"", vec!["+1 202 555 0142"]),
            ]
        );
    }

    #[test]
    fn google_csv_old_format_and_outlook() {
        let old = "Name,Given Name,Family Name,Phone 1 - Type,Phone 1 - Value\r\nCarla Lima,Carla,Lima,Mobile,81 9 8888-7777\r\n";
        assert_eq!(names(&parse(old.as_bytes())), [("Carla Lima", vec!["81 9 8888-7777"])]);
        let outlook = "First Name,Last Name,Mobile Phone,Home Phone\nDiego,,+351 912 345 678,\n";
        assert_eq!(names(&parse(outlook.as_bytes())), [("Diego", vec!["+351 912 345 678"])]);
    }

    #[test]
    fn vcard_with_folding_groups_and_escapes() {
        let vcf = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:Ana Souza\r\nN:Souza;Ana;;;\r\nitem1.TEL;TYPE=CELL:+55 81 99941-0815\r\nTEL;TYPE=HOME:(81) 3333\r\n -4444\r\nEND:VCARD\r\n\
                   BEGIN:VCARD\nVERSION:3.0\nN:Lima;Carla;;Dra.;\nTEL:tel:+5581988887777\nEND:VCARD\n\
                   BEGIN:VCARD\nFN:Loja\\, Centro\nTEL:3222\nEND:VCARD\n\
                   BEGIN:VCARD\nFN:Sem número\nEND:VCARD\n";
        assert_eq!(
            names(&parse(vcf.as_bytes())),
            [
                ("Ana Souza", vec!["+55 81 99941-0815", "(81) 3333-4444"]),
                ("Dra. Carla Lima", vec!["+5581988887777"]),
                ("Loja, Centro", vec!["3222"]),
            ]
        );
    }

    #[test]
    fn numbers_as_whatsapp_writes_them() {
        let br = Some("55");
        assert_eq!(normalize("+55 (81) 99941-0815", br).as_deref(), Some("5581999410815"));
        assert_eq!(normalize("(81) 99941-0815", br).as_deref(), Some("5581999410815"));
        assert_eq!(normalize("81 9941-0815", br).as_deref(), Some("558199410815"));
        assert_eq!(normalize("081 99941-0815", br).as_deref(), Some("5581999410815"));
        assert_eq!(normalize("0 15 81 99941-0815", br).as_deref(), Some("5581999410815"));
        assert_eq!(normalize("5581999410815", br).as_deref(), Some("5581999410815"));
        // DDD 55 (Rio Grande do Sul) in national format is not mistaken for the country code.
        assert_eq!(normalize("(55) 99999-1234", br).as_deref(), Some("5555999991234"));
        assert_eq!(normalize("00 1 202 555 0142", br).as_deref(), Some("12025550142"));
        assert_eq!(normalize("+1 202-555-0142", None).as_deref(), Some("12025550142"));
        assert_eq!(normalize("190", br), None, "too short");
        assert_eq!(normalize("ramal", br), None);
    }

    #[test]
    fn brazilian_ninth_digit() {
        assert_eq!(variants("5581999410815"), ["5581999410815", "558199410815"]);
        assert_eq!(variants("558199410815"), ["558199410815", "5581999410815"]);
        // Landlines (2–5) get no "9".
        assert_eq!(variants("558133334444"), ["558133334444"]);
        assert_eq!(variants("12025550142"), ["12025550142"]);
    }

    #[test]
    fn country_code_of_the_history() {
        assert_eq!(infer_country_code(["5581999410815", "5511911111111", "12025550142"]).as_deref(), Some("55"));
        assert_eq!(infer_country_code(["12025550142", "13105550123"]).as_deref(), Some("1"));
        assert_eq!(infer_country_code(["5581999410815", "12025550142", "447700900123", "351912345678"]), None);
        assert_eq!(infer_country_code([]), None);
    }
}
