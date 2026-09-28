//! LandXML 1.2 TIN surface import + export (std + serde only).
//!
//! Import convention (per OpenCADStudio issue #157): a `<P>` element carries
//! `northing easting [elev]` text → world `X = Easting, Y = Northing, Z = Z`.
//! Attribute-form points (`<P x="E" y="N" z="Z"/>`, as written by our own
//! exporter) are read as `X = x, Y = y`. Faces reference point ids; invisible
//! `<F i="1">` faces are skipped.

use crate::surface::Surface;

/// A named TIN surface read from (or written to) LandXML.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedSurface {
    pub name: String,
    pub surface: Surface,
}

/// Quick heuristic: does `text` look like a LandXML document?
pub fn looks_like_landxml(text: &str) -> bool {
    text.contains("<LandXML")
}

/// Read every TIN surface in a LandXML document.
pub fn read_surfaces(text: &str) -> Vec<NamedSurface> {
    let mut out = Vec::new();
    let mut search = 0usize;
    while let Some(start) = find_tag(text, search, "Surface") {
        let open_end = match text[start..].find('>') {
            Some(i) => start + i,
            None => break,
        };
        let open_tag = &text[start..=open_end];
        // Skip container tags like <Surfaces>.
        if open_tag.starts_with("<Surfaces") {
            search = open_end + 1;
            continue;
        }
        let close = match text[open_end..].find("</Surface>") {
            Some(i) => open_end + i + "</Surface>".len(),
            None => break,
        };
        let block = &text[start..close];
        if let Some(ns) = parse_surface_block(block, open_tag) {
            out.push(ns);
        }
        search = close;
    }
    out
}

/// Read the first TIN surface in a LandXML document.
pub fn read_first_surface(text: &str) -> Option<NamedSurface> {
    read_surfaces(text).into_iter().next()
}

/// Alias kept for older call sites / docs.
pub fn parse_surface(text: &str) -> Option<NamedSurface> {
    read_first_surface(text)
}

fn find_tag(text: &str, from: usize, name: &str) -> Option<usize> {
    let mut i = from;
    while i < text.len() {
        let rel = text[i..].find('<')?;
        i += rel;
        let rest = &text[i..];
        // Must be `<Name` followed by whitespace, `>`, or `/`.
        if rest.len() > name.len() + 1 && rest[1..1 + name.len()] == *name {
            let c = rest[1 + name.len()..].chars().next().unwrap_or(' ');
            if c == ' ' || c == '>' || c == '/' || c == '\t' || c == '\n' || c == '\r' {
                // Exclude closing tags `</Name`.
                if rest.starts_with("</") {
                    i += 1;
                    continue;
                }
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn parse_surface_block(block: &str, open_tag: &str) -> Option<NamedSurface> {
    let raw_name = parse_attr(open_tag, "name").unwrap_or_else(|| "SURFACE".to_string());
    let name = xml_unescape(&raw_name);
    let pnts = extract_section(block, "Pnts")?;
    let faces = extract_section(block, "Faces").unwrap_or_default();
    let (nodes, id_to_idx) = parse_points(&pnts);
    if nodes.is_empty() {
        return None;
    }
    let triangles = parse_faces(&faces, &id_to_idx);
    if triangles.is_empty() {
        return None;
    }
    let mut surface = Surface {
        nodes,
        triangles,
        ..Default::default()
    };
    surface.name = name.clone();
    Some(NamedSurface { name, surface })
}

fn extract_section(block: &str, section: &str) -> Option<String> {
    let open = format!("<{section}");
    let close = format!("</{section}>");
    let s = block.find(open.as_str())?;
    let body_start = block[s..].find('>')? + s + 1;
    let e = block[body_start..].find(close.as_str())?;
    Some(block[body_start..body_start + e].to_string())
}

fn parse_points(pnts: &str) -> (Vec<[f64; 3]>, std::collections::HashMap<i64, usize>) {
    let mut nodes: Vec<[f64; 3]> = Vec::new();
    let mut map: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    let mut i = 0usize;
    while i < pnts.len() {
        let rel = match pnts[i..].find("<P") {
            Some(r) => r,
            None => break,
        };
        i += rel;
        // Avoid matching <Pnts>.
        if pnts[i..].starts_with("<Pnts") {
            i += 5;
            continue;
        }
        let tag_end = match pnts[i..].find('>') {
            Some(e) => i + e,
            None => break,
        };
        let tag = &pnts[i..=tag_end];
        let self_closing = tag.ends_with("/>");
        let id: i64 = parse_attr(tag, "id")
            .and_then(|v| v.parse().ok())
            .unwrap_or(nodes.len() as i64);
        // Attribute-form coordinates take precedence when present.
        let ax = parse_attr(tag, "x").and_then(|v| v.parse::<f64>().ok());
        let ay = parse_attr(tag, "y").and_then(|v| v.parse::<f64>().ok());
        let az = parse_attr(tag, "z").and_then(|v| v.parse::<f64>().ok());
        if self_closing {
            if let (Some(x), Some(y)) = (ax, ay) {
                let z = az.unwrap_or(0.0);
                map.insert(id, nodes.len());
                nodes.push([x, y, z]);
            }
            i = tag_end + 1;
            continue;
        }
        // Content form: `<P ...>northing easting [elev]</P>`.
        let close = match pnts[tag_end..].find("</P>") {
            Some(e) => tag_end + e,
            None => break,
        };
        let content = pnts[tag_end + 1..close].trim();
        if let (Some(x), Some(y)) = (ax, ay) {
            let z = az.unwrap_or_else(|| {
                content
                    .split_whitespace()
                    .nth(2)
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0.0)
            });
            map.insert(id, nodes.len());
            nodes.push([x, y, z]);
        } else {
            let nums: Vec<f64> = content
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            if nums.len() >= 2 {
                let (northing, easting) = (nums[0], nums[1]);
                let elev = if nums.len() >= 3 { nums[2] } else { 0.0 };
                map.insert(id, nodes.len());
                nodes.push([easting, northing, elev]);
            }
        }
        i = close + "</P>".len();
    }
    (nodes, map)
}

fn parse_faces(faces: &str, id_to_idx: &std::collections::HashMap<i64, usize>) -> Vec<[usize; 3]> {
    let mut tris = Vec::new();
    let mut i = 0usize;
    while i < faces.len() {
        let rel = match faces[i..].find("<F") {
            Some(r) => r,
            None => break,
        };
        i += rel;
        if faces[i..].starts_with("<Faces") {
            i += 6;
            continue;
        }
        let tag_end = match faces[i..].find('>') {
            Some(e) => i + e,
            None => break,
        };
        let tag = &faces[i..=tag_end];
        // Invisible faces (e.g. `<F i="1">`) are skipped.
        if parse_attr(tag, "i").as_deref() == Some("1") {
            if tag.ends_with("/>") {
                i = tag_end + 1;
            } else if let Some(e) = faces[tag_end..].find("</F>") {
                i = tag_end + e + "</F>".len();
            } else {
                break;
            }
            continue;
        }
        if tag.ends_with("/>") {
            i = tag_end + 1;
            continue;
        }
        let close = match faces[tag_end..].find("</F>") {
            Some(e) => tag_end + e,
            None => break,
        };
        let content = faces[tag_end + 1..close].trim();
        let ids: Vec<i64> = content
            .split_whitespace()
            .filter_map(|v| v.parse().ok())
            .collect();
        if ids.len() == 3 {
            if let (Some(&a), Some(&b), Some(&c)) = (
                id_to_idx.get(&ids[0]),
                id_to_idx.get(&ids[1]),
                id_to_idx.get(&ids[2]),
            ) {
                tris.push([a, b, c]);
            }
        }
        i = close + "</F>".len();
    }
    tris
}

fn parse_attr(tag: &str, key: &str) -> Option<String> {
    let needle = format!("{key}=");
    let mut search = 0usize;
    while let Some(pos) = tag[search..].find(needle.as_str()) {
        let abs = search + pos;
        // Attribute name must start at a boundary (whitespace or '<').
        if abs > 0 {
            let prev = tag[..abs].chars().next_back().unwrap_or(' ');
            if !(prev == ' ' || prev == '\t' || prev == '\n' || prev == '\r' || prev == '<') {
                search = abs + 1;
                continue;
            }
        }
        let after = &tag[abs + needle.len()..];
        let quote = after.chars().next()?;
        if quote != '"' && quote != '\'' {
            search = abs + 1;
            continue;
        }
        let rest = &after[1..];
        let end = rest.find(quote)?;
        return Some(rest[..end].to_string());
    }
    None
}

fn xml_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

// ---------- LandXML 1.2 export ----------

/// Export a surface to a fully valid LandXML 1.2 document.
///
/// Points use 1-based ids in attribute form (`<P id x y z/>` with
/// `x = Easting, y = Northing`); faces are 1-based `<F>i1 i2 i3</F>`.
pub fn export_surface_to_landxml(surface: &Surface) -> String {
    use std::fmt::Write;

    let mut xml = String::new();
    writeln!(xml, r#"<?xml version="1.0" encoding="UTF-8"?>"#).unwrap();
    writeln!(
        xml,
        r#"<LandXML xmlns="http://www.landxml.org/schema/LandXML-1.2" version="1.2" date="{}">"#,
        chrono::Utc::now().format("%Y-%m-%d")
    )
    .unwrap();
    writeln!(xml, "  <Units>").unwrap();
    writeln!(
        xml,
        r#"    <Metric areaUnit="squareMeter" linearUnit="meter" volumeUnit="cubicMeter" temperatureUnit="celsius" pressureUnit="kPa"/>"#
    )
    .unwrap();
    writeln!(xml, "  </Units>").unwrap();
    let display_name = if surface.name.is_empty() {
        "SURFACE"
    } else {
        surface.name.as_str()
    };
    writeln!(xml, r#"  <Project name="{}">"#, xml_escape(display_name)).unwrap();
    writeln!(xml, "  </Project>").unwrap();
    writeln!(xml, "  <Surfaces>").unwrap();
    writeln!(xml, r#"    <Surface name="{}">"#, xml_escape(display_name)).unwrap();
    writeln!(xml, r#"      <Definition surfType="TIN">"#).unwrap();

    writeln!(xml, "        <Pnts>").unwrap();
    for (i, node) in surface.nodes.iter().enumerate() {
        writeln!(
            xml,
            r#"          <P id="{}" x="{:.6}" y="{:.6}" z="{:.6}"/>"#,
            i + 1,
            node[0],
            node[1],
            node[2]
        )
        .unwrap();
    }
    writeln!(xml, "        </Pnts>").unwrap();

    writeln!(xml, "        <Faces>").unwrap();
    for tri in &surface.triangles {
        writeln!(
            xml,
            "          <F>{} {} {}</F>",
            tri[0] + 1,
            tri[1] + 1,
            tri[2] + 1
        )
        .unwrap();
    }
    writeln!(xml, "        </Faces>").unwrap();

    writeln!(xml, "      </Definition>").unwrap();
    writeln!(xml, "    </Surface>").unwrap();
    writeln!(xml, "  </Surfaces>").unwrap();
    writeln!(xml, "</LandXML>").unwrap();

    xml
}

fn xml_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(ch),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn landxml_round_trip_preserves_topology() {
        let surf = Surface::from_points(&[
            [0.0, 0.0, 1.0],
            [10.0, 0.0, 2.0],
            [10.0, 10.0, 3.0],
            [0.0, 10.0, 4.0],
            [5.0, 5.0, 5.0],
        ]);
        let mut named = surf;
        named.name = "TEST & <SURFACE>".to_string();
        let xml = export_surface_to_landxml(&named);
        assert!(xml.contains("<LandXML"));
        assert!(xml.contains("surfType=\"TIN\""));
        assert!(xml.contains("squareMeter"));
        // Name is XML-escaped.
        assert!(xml.contains("TEST &amp; &lt;SURFACE&gt;"));
        let back = read_first_surface(&xml).expect("round-trip parse");
        assert_eq!(back.surface.nodes.len(), named.nodes.len());
        assert_eq!(back.surface.triangles.len(), named.triangles.len());
        assert_eq!(back.name, named.name);
        for (a, b) in back.surface.nodes.iter().zip(named.nodes.iter()) {
            assert!((a[0] - b[0]).abs() < 1e-6);
            assert!((a[1] - b[1]).abs() < 1e-6);
            assert!((a[2] - b[2]).abs() < 1e-6);
        }
    }

    #[test]
    fn content_form_northing_easting_maps_to_xy() {
        let xml = r#"<?xml version="1.0"?>
<LandXML version="1.2">
<Surfaces><Surface name="S"><Definition surfType="TIN">
<Pnts><P id="0">10.0 20.0 5.0</P></Pnts>
<Faces><F>0 1 2</F></Faces>
</Definition></Surface></Surfaces></LandXML>"#;
        // Dangling face references are dropped, so no surface survives.
        assert!(looks_like_landxml(xml));
        assert!(read_first_surface(xml).is_none());
        // A well-formed single triangle maps N/E content to X=E/Y=N.
        let xml2 = r#"<?xml version="1.0"?>
<LandXML version="1.2">
<Surfaces><Surface name="S"><Definition surfType="TIN">
<Pnts><P id="0">10.0 20.0 5.0</P><P id="1">10.0 30.0 6.0</P><P id="2">20.0 20.0 7.0</P></Pnts>
<Faces><F>0 1 2</F></Faces>
</Definition></Surface></Surfaces></LandXML>"#;
        let ns = read_first_surface(xml2).expect("triangle");
        assert_eq!(ns.surface.nodes[0], [20.0, 10.0, 5.0]);
    }
}
