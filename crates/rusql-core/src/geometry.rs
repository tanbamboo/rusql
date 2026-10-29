//! MySQL 8.0 geometry subset (M196): WKT, SRID, and little-endian WKB payload.
//!
//! Cells store lowercase hex of `SRID` (4 LE bytes) plus ISO WKB. Ordinates are the
//! order `ST_AsText` prints. SRID 4326 is geographic (lat, lon) with range checks.
//! Switching between 4326 and a non-geographic SRID swaps axes. SRID 3857 is stored
//! and is not projected.

use std::fmt::Write as _;

const MAX_COORDS: usize = 65_536;

/// Why a geometry value or column was rejected.
#[derive(Debug, Clone, PartialEq)]
pub enum GeometryFault {
    InvalidData,
    UnexpectedType {
        expected: &'static str,
        actual: &'static str,
    },
    SrsNotFound(u32),
    NotGeometry,
    TypeMismatch,
    SridMismatch {
        geometry: u32,
        column_srid: u32,
    },
    NotGeographic {
        srid: u32,
    },
    LatitudeOutOfRange(f64),
    LongitudeOutOfRange(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SpatialKind {
    Geometry,
    Point,
    LineString,
    Polygon,
    MultiPoint,
    MultiLineString,
    MultiPolygon,
    GeometryCollection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedGeom {
    Point,
    LineString,
    Polygon,
    MultiPoint,
    MultiLineString,
    MultiPolygon,
    GeometryCollection,
}

#[derive(Debug, Clone, Copy)]
struct SpatialColumn {
    kind: SpatialKind,
    srid: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Coord {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum Geometry {
    Point(Coord),
    LineString(Vec<Coord>),
    Polygon(Vec<Vec<Coord>>),
    MultiPoint(Vec<Coord>),
    MultiLineString(Vec<Vec<Coord>>),
    MultiPolygon(Vec<Vec<Vec<Coord>>>),
    Collection(Vec<Geometry>),
}

struct GeomValue {
    srid: u32,
    geom: Geometry,
}

/// `POINT SRID n` column, or `None` when the catalog type is not spatial.
fn spatial_column(data_type: &str) -> Option<SpatialColumn> {
    let upper = data_type.trim().to_uppercase();
    let mut parts = upper.split_whitespace();
    let name = parts.next()?;
    let kind = spatial_kind_name(name)?;
    let srid = match (parts.next(), parts.next(), parts.next()) {
        (None, _, _) => None,
        (Some("SRID"), Some(n), None) => Some(n.parse().ok()?),
        _ => return None,
    };
    Some(SpatialColumn { kind, srid })
}

fn spatial_kind_name(name: &str) -> Option<SpatialKind> {
    match name {
        "GEOMETRY" => Some(SpatialKind::Geometry),
        "POINT" => Some(SpatialKind::Point),
        "LINESTRING" => Some(SpatialKind::LineString),
        "POLYGON" => Some(SpatialKind::Polygon),
        "MULTIPOINT" => Some(SpatialKind::MultiPoint),
        "MULTILINESTRING" => Some(SpatialKind::MultiLineString),
        "MULTIPOLYGON" => Some(SpatialKind::MultiPolygon),
        "GEOMETRYCOLLECTION" | "GEOMCOLLECTION" => Some(SpatialKind::GeometryCollection),
        _ => None,
    }
}

fn column_catalog_name(kind: SpatialKind) -> &'static str {
    match kind {
        SpatialKind::Geometry => "GEOMETRY",
        SpatialKind::Point => "POINT",
        SpatialKind::LineString => "LINESTRING",
        SpatialKind::Polygon => "POLYGON",
        SpatialKind::MultiPoint => "MULTIPOINT",
        SpatialKind::MultiLineString => "MULTILINESTRING",
        SpatialKind::MultiPolygon => "MULTIPOLYGON",
        SpatialKind::GeometryCollection => "GEOMCOLLECTION",
    }
}

/// Fold `POINT(4326)` into `POINT SRID 4326` and `GEOMETRYCOLLECTION` into `GEOMCOLLECTION`.
pub(crate) fn canonicalize_spatial_type_name(normalized: &str) -> String {
    let trimmed = normalized.trim();
    if let Some((base, rest)) = trimmed.split_once('(') {
        let base = base.trim();
        if spatial_kind_name(base).is_some() && !rest.contains(',') {
            let inner = rest.trim().trim_end_matches(')').trim();
            if !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit()) {
                let kind = spatial_kind_name(base).expect("checked");
                return format!("{} SRID {inner}", column_catalog_name(kind));
            }
        }
    }
    if let Some(kind) = spatial_kind_name(trimmed) {
        return column_catalog_name(kind).to_string();
    }
    trimmed.to_string()
}

pub(crate) fn spatial_type_base(data_type: &str) -> Option<&'static str> {
    spatial_column(data_type).map(|c| column_catalog_name(c.kind))
}

pub(crate) fn spatial_show_create(data_type: &str) -> Option<String> {
    let col = spatial_column(data_type)?;
    let mut s = column_catalog_name(col.kind).to_string();
    if let Some(srid) = col.srid {
        let _ = write!(s, " /*!80003 SRID {srid} */");
    }
    Some(s)
}

/// Unknown column SRID is errno 3548. Non-spatial types are fine.
pub fn validate_spatial_column(data_type: &str) -> Result<(), GeometryFault> {
    if let Some(srid) = spatial_column(data_type).and_then(|c| c.srid) {
        check_srid(srid)?;
    }
    Ok(())
}

pub fn is_known_srid(srid: u32) -> bool {
    matches!(srid, 0 | 4326 | 3857)
}

fn check_srid(srid: u32) -> Result<(), GeometryFault> {
    if is_known_srid(srid) {
        Ok(())
    } else {
        Err(GeometryFault::SrsNotFound(srid))
    }
}

fn is_geographic(srid: u32) -> bool {
    srid == 4326
}

/// Parse WKT and return the stored hex payload.
pub fn geom_from_wkt(
    wkt: &str,
    srid: u32,
    expect: Option<ExpectedGeom>,
) -> Result<String, GeometryFault> {
    check_srid(srid)?;
    let geom = parse_wkt(wkt).map_err(|_| GeometryFault::InvalidData)?;
    if let Some(expect) = expect {
        if !expect.matches(&geom) {
            return Err(GeometryFault::UnexpectedType {
                expected: "WKT",
                actual: type_name(&geom),
            });
        }
    }
    if is_geographic(srid) {
        check_geographic_range(&geom)?;
    }
    Ok(encode(srid, &geom))
}

pub fn geom_as_text(payload: &str) -> Result<String, GeometryFault> {
    let value = decode(payload)?;
    Ok(to_wkt(&value.geom))
}

pub fn geom_srid(payload: &str) -> Result<u32, GeometryFault> {
    Ok(decode(payload)?.srid)
}

pub fn geom_set_srid(payload: &str, srid: u32) -> Result<String, GeometryFault> {
    check_srid(srid)?;
    let mut value = decode(payload)?;
    if is_geographic(value.srid) != is_geographic(srid) {
        swap_axes(&mut value.geom);
    }
    if is_geographic(srid) {
        check_geographic_range(&value.geom)?;
    }
    Ok(encode(srid, &value.geom))
}

pub fn geom_type_name(payload: &str) -> Result<&'static str, GeometryFault> {
    Ok(type_name(&decode(payload)?.geom))
}

pub fn geom_dimension(payload: &str) -> Result<i32, GeometryFault> {
    Ok(dimension(&decode(payload)?.geom))
}

pub fn geom_is_empty(payload: &str) -> Result<bool, GeometryFault> {
    let _ = decode(payload)?;
    Ok(false)
}

pub fn geom_xy(payload: &str) -> Result<(f64, f64), GeometryFault> {
    let value = decode(payload)?;
    match value.geom {
        Geometry::Point(c) => Ok((c.x, c.y)),
        other => Err(GeometryFault::UnexpectedType {
            expected: "POINT",
            actual: type_name(&other),
        }),
    }
}

pub fn geom_latitude(payload: &str) -> Result<f64, GeometryFault> {
    geographic_point(payload).map(|c| c.x)
}

pub fn geom_longitude(payload: &str) -> Result<f64, GeometryFault> {
    geographic_point(payload).map(|c| c.y)
}

fn geographic_point(payload: &str) -> Result<Coord, GeometryFault> {
    let value = decode(payload)?;
    let srid = value.srid;
    let actual = type_name(&value.geom);
    let Geometry::Point(c) = value.geom else {
        return Err(GeometryFault::UnexpectedType {
            expected: "POINT",
            actual,
        });
    };
    if !is_geographic(srid) {
        return Err(GeometryFault::NotGeographic { srid });
    }
    Ok(c)
}

/// Insert/update into a spatial column. Empty text is SQL NULL.
pub fn store_spatial_cell(data_type: &str, value: &str) -> Result<String, GeometryFault> {
    let Some(col) = spatial_column(data_type) else {
        return Ok(value.to_string());
    };
    if value.is_empty() {
        return Ok(String::new());
    }
    let parsed = decode(value).map_err(|_| GeometryFault::NotGeometry)?;
    check_srid(parsed.srid)?;
    if !col.kind.accepts(&parsed.geom) {
        return Err(GeometryFault::TypeMismatch);
    }
    if let Some(want) = col.srid {
        if want != parsed.srid {
            return Err(GeometryFault::SridMismatch {
                geometry: parsed.srid,
                column_srid: want,
            });
        }
    }
    Ok(encode(parsed.srid, &parsed.geom))
}

pub fn format_ordinate(n: f64) -> String {
    if n.to_bits() == 0 || n.to_bits() == (-0.0f64).to_bits() {
        "0".to_string()
    } else {
        format!("{n}")
    }
}

impl ExpectedGeom {
    fn matches(self, geom: &Geometry) -> bool {
        matches!(
            (self, geom),
            (Self::Point, Geometry::Point(_))
                | (Self::LineString, Geometry::LineString(_))
                | (Self::Polygon, Geometry::Polygon(_))
                | (Self::MultiPoint, Geometry::MultiPoint(_))
                | (Self::MultiLineString, Geometry::MultiLineString(_))
                | (Self::MultiPolygon, Geometry::MultiPolygon(_))
                | (Self::GeometryCollection, Geometry::Collection(_))
        )
    }
}

impl SpatialKind {
    fn accepts(self, geom: &Geometry) -> bool {
        match self {
            Self::Geometry => true,
            Self::Point => matches!(geom, Geometry::Point(_)),
            Self::LineString => matches!(geom, Geometry::LineString(_)),
            Self::Polygon => matches!(geom, Geometry::Polygon(_)),
            Self::MultiPoint => matches!(geom, Geometry::MultiPoint(_)),
            Self::MultiLineString => matches!(geom, Geometry::MultiLineString(_)),
            Self::MultiPolygon => matches!(geom, Geometry::MultiPolygon(_)),
            Self::GeometryCollection => matches!(geom, Geometry::Collection(_)),
        }
    }
}

fn type_name(geom: &Geometry) -> &'static str {
    match geom {
        Geometry::Point(_) => "POINT",
        Geometry::LineString(_) => "LINESTRING",
        Geometry::Polygon(_) => "POLYGON",
        Geometry::MultiPoint(_) => "MULTIPOINT",
        Geometry::MultiLineString(_) => "MULTILINESTRING",
        Geometry::MultiPolygon(_) => "MULTIPOLYGON",
        Geometry::Collection(_) => "GEOMETRYCOLLECTION",
    }
}

fn dimension(geom: &Geometry) -> i32 {
    match geom {
        Geometry::Point(_) | Geometry::MultiPoint(_) => 0,
        Geometry::LineString(_) | Geometry::MultiLineString(_) => 1,
        Geometry::Polygon(_) | Geometry::MultiPolygon(_) => 2,
        Geometry::Collection(parts) => parts.iter().map(dimension).max().unwrap_or(-1),
    }
}

fn swap_axes(geom: &mut Geometry) {
    match geom {
        Geometry::Point(c) => c.swap(),
        Geometry::LineString(pts) | Geometry::MultiPoint(pts) => {
            for c in pts {
                c.swap();
            }
        }
        Geometry::Polygon(rings) | Geometry::MultiLineString(rings) => {
            for ring in rings {
                for c in ring {
                    c.swap();
                }
            }
        }
        Geometry::MultiPolygon(polys) => {
            for poly in polys {
                for ring in poly {
                    for c in ring {
                        c.swap();
                    }
                }
            }
        }
        Geometry::Collection(parts) => {
            for part in parts {
                swap_axes(part);
            }
        }
    }
}

impl Coord {
    fn swap(&mut self) {
        std::mem::swap(&mut self.x, &mut self.y);
    }
}

fn check_geographic_range(geom: &Geometry) -> Result<(), GeometryFault> {
    fn check(c: &Coord) -> Result<(), GeometryFault> {
        if !(-90.0..=90.0).contains(&c.x) {
            return Err(GeometryFault::LatitudeOutOfRange(c.x));
        }
        if c.y <= -180.0 || c.y > 180.0 {
            return Err(GeometryFault::LongitudeOutOfRange(c.y));
        }
        Ok(())
    }
    fn walk(geom: &Geometry) -> Result<(), GeometryFault> {
        match geom {
            Geometry::Point(c) => check(c),
            Geometry::LineString(pts) | Geometry::MultiPoint(pts) => pts.iter().try_for_each(check),
            Geometry::Polygon(rings) | Geometry::MultiLineString(rings) => {
                rings.iter().flatten().try_for_each(check)
            }
            Geometry::MultiPolygon(polys) => polys.iter().flatten().flatten().try_for_each(check),
            Geometry::Collection(parts) => parts.iter().try_for_each(walk),
        }
    }
    walk(geom)
}

fn encode(srid: u32, geom: &Geometry) -> String {
    let mut buf = Vec::new();
    buf.extend_from_slice(&srid.to_le_bytes());
    write_wkb(&mut buf, geom);
    encode_hex(&buf)
}

fn decode(payload: &str) -> Result<GeomValue, GeometryFault> {
    let bytes = decode_hex(payload).ok_or(GeometryFault::InvalidData)?;
    if bytes.len() < 9 {
        return Err(GeometryFault::InvalidData);
    }
    let srid = u32::from_le_bytes(bytes[0..4].try_into().expect("4 bytes"));
    let mut i = 4;
    let geom = read_wkb(&bytes, &mut i)?;
    if i != bytes.len() {
        return Err(GeometryFault::InvalidData);
    }
    Ok(GeomValue { srid, geom })
}

fn write_wkb(buf: &mut Vec<u8>, geom: &Geometry) {
    buf.push(1);
    buf.extend_from_slice(&wkb_type(geom).to_le_bytes());
    match geom {
        Geometry::Point(c) => write_coord(buf, c),
        Geometry::LineString(pts) => write_coord_seq(buf, pts),
        Geometry::Polygon(rings) => {
            push_u32(buf, rings.len() as u32);
            for ring in rings {
                write_coord_seq(buf, ring);
            }
        }
        Geometry::MultiPoint(pts) => {
            push_u32(buf, pts.len() as u32);
            for c in pts {
                write_wkb(buf, &Geometry::Point(*c));
            }
        }
        Geometry::MultiLineString(lines) => {
            push_u32(buf, lines.len() as u32);
            for line in lines {
                buf.push(1);
                buf.extend_from_slice(&2u32.to_le_bytes());
                write_coord_seq(buf, line);
            }
        }
        Geometry::MultiPolygon(polys) => {
            push_u32(buf, polys.len() as u32);
            for poly in polys {
                buf.push(1);
                buf.extend_from_slice(&3u32.to_le_bytes());
                push_u32(buf, poly.len() as u32);
                for ring in poly {
                    write_coord_seq(buf, ring);
                }
            }
        }
        Geometry::Collection(parts) => {
            push_u32(buf, parts.len() as u32);
            for part in parts {
                write_wkb(buf, part);
            }
        }
    }
}

fn wkb_type(geom: &Geometry) -> u32 {
    match geom {
        Geometry::Point(_) => 1,
        Geometry::LineString(_) => 2,
        Geometry::Polygon(_) => 3,
        Geometry::MultiPoint(_) => 4,
        Geometry::MultiLineString(_) => 5,
        Geometry::MultiPolygon(_) => 6,
        Geometry::Collection(_) => 7,
    }
}

fn write_coord(buf: &mut Vec<u8>, c: &Coord) {
    buf.extend_from_slice(&c.x.to_le_bytes());
    buf.extend_from_slice(&c.y.to_le_bytes());
}

fn write_coord_seq(buf: &mut Vec<u8>, pts: &[Coord]) {
    push_u32(buf, pts.len() as u32);
    for c in pts {
        write_coord(buf, c);
    }
}

fn push_u32(buf: &mut Vec<u8>, n: u32) {
    buf.extend_from_slice(&n.to_le_bytes());
}

fn read_wkb(bytes: &[u8], i: &mut usize) -> Result<Geometry, GeometryFault> {
    if *i >= bytes.len() || bytes[*i] != 1 {
        return Err(GeometryFault::InvalidData);
    }
    *i += 1;
    let ty = read_u32(bytes, i)?;
    match ty {
        1 => Ok(Geometry::Point(read_coord(bytes, i)?)),
        2 => Ok(Geometry::LineString(read_coord_seq(bytes, i, 2)?)),
        3 => {
            let n = read_count(bytes, i)?;
            let mut rings = Vec::with_capacity(n);
            for _ in 0..n {
                rings.push(read_coord_seq(bytes, i, 4)?);
            }
            Ok(Geometry::Polygon(rings))
        }
        4 => {
            let n = read_count(bytes, i)?;
            let mut pts = Vec::with_capacity(n);
            for _ in 0..n {
                match read_wkb(bytes, i)? {
                    Geometry::Point(c) => pts.push(c),
                    _ => return Err(GeometryFault::InvalidData),
                }
            }
            Ok(Geometry::MultiPoint(pts))
        }
        5 => {
            let n = read_count(bytes, i)?;
            let mut lines = Vec::with_capacity(n);
            for _ in 0..n {
                match read_wkb(bytes, i)? {
                    Geometry::LineString(pts) => lines.push(pts),
                    _ => return Err(GeometryFault::InvalidData),
                }
            }
            Ok(Geometry::MultiLineString(lines))
        }
        6 => {
            let n = read_count(bytes, i)?;
            let mut polys = Vec::with_capacity(n);
            for _ in 0..n {
                match read_wkb(bytes, i)? {
                    Geometry::Polygon(rings) => polys.push(rings),
                    _ => return Err(GeometryFault::InvalidData),
                }
            }
            Ok(Geometry::MultiPolygon(polys))
        }
        7 => {
            let n = read_count(bytes, i)?;
            let mut parts = Vec::with_capacity(n);
            for _ in 0..n {
                parts.push(read_wkb(bytes, i)?);
            }
            Ok(Geometry::Collection(parts))
        }
        _ => Err(GeometryFault::InvalidData),
    }
}

fn read_count(bytes: &[u8], i: &mut usize) -> Result<usize, GeometryFault> {
    let n = read_u32(bytes, i)? as usize;
    if n == 0 || n > MAX_COORDS {
        return Err(GeometryFault::InvalidData);
    }
    Ok(n)
}

fn read_coord_seq(bytes: &[u8], i: &mut usize, min: usize) -> Result<Vec<Coord>, GeometryFault> {
    let n = read_count(bytes, i)?;
    if n < min {
        return Err(GeometryFault::InvalidData);
    }
    let mut pts = Vec::with_capacity(n);
    for _ in 0..n {
        pts.push(read_coord(bytes, i)?);
    }
    Ok(pts)
}

fn read_coord(bytes: &[u8], i: &mut usize) -> Result<Coord, GeometryFault> {
    if *i + 16 > bytes.len() {
        return Err(GeometryFault::InvalidData);
    }
    let x = f64::from_le_bytes(bytes[*i..*i + 8].try_into().expect("8 bytes"));
    *i += 8;
    let y = f64::from_le_bytes(bytes[*i..*i + 8].try_into().expect("8 bytes"));
    *i += 8;
    if !x.is_finite() || !y.is_finite() {
        return Err(GeometryFault::InvalidData);
    }
    Ok(Coord { x, y })
}

fn read_u32(bytes: &[u8], i: &mut usize) -> Result<u32, GeometryFault> {
    if *i + 4 > bytes.len() {
        return Err(GeometryFault::InvalidData);
    }
    let n = u32::from_le_bytes(bytes[*i..*i + 4].try_into().expect("4 bytes"));
    *i += 4;
    Ok(n)
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.is_empty() || s.len() % 2 != 0 {
        return None;
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i])?;
        let lo = hex_val(bytes[i + 1])?;
        out.push((hi << 4) | lo);
        i += 2;
    }
    Some(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn to_wkt(geom: &Geometry) -> String {
    let mut out = String::new();
    write_geom(&mut out, geom);
    out
}

fn write_geom(out: &mut String, geom: &Geometry) {
    match geom {
        Geometry::Point(c) => {
            out.push_str("POINT(");
            write_coord_text(out, c);
            out.push(')');
        }
        Geometry::LineString(pts) => {
            out.push_str("LINESTRING(");
            write_coords(out, pts);
            out.push(')');
        }
        Geometry::Polygon(rings) => {
            out.push_str("POLYGON(");
            write_rings(out, rings);
            out.push(')');
        }
        Geometry::MultiPoint(pts) => {
            out.push_str("MULTIPOINT(");
            for (i, c) in pts.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('(');
                write_coord_text(out, c);
                out.push(')');
            }
            out.push(')');
        }
        Geometry::MultiLineString(lines) => {
            out.push_str("MULTILINESTRING(");
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('(');
                write_coords(out, line);
                out.push(')');
            }
            out.push(')');
        }
        Geometry::MultiPolygon(polys) => {
            out.push_str("MULTIPOLYGON(");
            for (i, poly) in polys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push('(');
                write_rings(out, poly);
                out.push(')');
            }
            out.push(')');
        }
        Geometry::Collection(parts) => {
            out.push_str("GEOMETRYCOLLECTION(");
            for (i, part) in parts.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_geom(out, part);
            }
            out.push(')');
        }
    }
}

fn write_rings(out: &mut String, rings: &[Vec<Coord>]) {
    for (i, ring) in rings.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('(');
        write_coords(out, ring);
        out.push(')');
    }
}

fn write_coords(out: &mut String, pts: &[Coord]) {
    for (i, c) in pts.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_coord_text(out, c);
    }
}

fn write_coord_text(out: &mut String, c: &Coord) {
    out.push_str(&format_ordinate(c.x));
    out.push(' ');
    out.push_str(&format_ordinate(c.y));
}

fn parse_wkt(input: &str) -> Result<Geometry, ()> {
    let mut p = Wkt {
        s: input.as_bytes(),
        i: 0,
    };
    let geom = p.parse_geometry()?;
    p.skip_ws();
    if p.i != p.s.len() {
        return Err(());
    }
    Ok(geom)
}

struct Wkt<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Wkt<'a> {
    fn parse_geometry(&mut self) -> Result<Geometry, ()> {
        let name = self.ident()?;
        self.skip_ws();
        if self.starts_with_word("EMPTY") {
            return Err(());
        }
        self.expect(b'(')?;
        let geom = match name.as_str() {
            "POINT" => Geometry::Point(self.coord()?),
            "LINESTRING" => Geometry::LineString(self.coord_list(2)?),
            "POLYGON" => Geometry::Polygon(self.rings()?),
            "MULTIPOINT" => Geometry::MultiPoint(self.multipoint()?),
            "MULTILINESTRING" => Geometry::MultiLineString(self.multi_lines()?),
            "MULTIPOLYGON" => Geometry::MultiPolygon(self.multi_polygons()?),
            "GEOMETRYCOLLECTION" | "GEOMCOLLECTION" => Geometry::Collection(self.collection()?),
            _ => return Err(()),
        };
        self.expect(b')')?;
        Ok(geom)
    }

    fn multipoint(&mut self) -> Result<Vec<Coord>, ()> {
        self.skip_ws();
        if self.peek() == Some(b'(') {
            let mut pts = Vec::new();
            loop {
                self.expect(b'(')?;
                pts.push(self.coord()?);
                self.expect(b')')?;
                self.skip_ws();
                if self.peek() == Some(b',') {
                    self.i += 1;
                    continue;
                }
                break;
            }
            if pts.is_empty() || pts.len() > MAX_COORDS {
                return Err(());
            }
            Ok(pts)
        } else {
            self.coord_list(1)
        }
    }

    fn multi_lines(&mut self) -> Result<Vec<Vec<Coord>>, ()> {
        let mut lines = Vec::new();
        loop {
            self.expect(b'(')?;
            lines.push(self.coord_list(2)?);
            self.expect(b')')?;
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.i += 1;
                continue;
            }
            break;
        }
        if lines.is_empty() {
            return Err(());
        }
        Ok(lines)
    }

    fn multi_polygons(&mut self) -> Result<Vec<Vec<Vec<Coord>>>, ()> {
        let mut polys = Vec::new();
        loop {
            self.expect(b'(')?;
            polys.push(self.rings()?);
            self.expect(b')')?;
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.i += 1;
                continue;
            }
            break;
        }
        if polys.is_empty() {
            return Err(());
        }
        Ok(polys)
    }

    fn collection(&mut self) -> Result<Vec<Geometry>, ()> {
        let mut parts = vec![self.parse_geometry()?];
        loop {
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.i += 1;
                parts.push(self.parse_geometry()?);
            } else {
                break;
            }
        }
        Ok(parts)
    }

    fn rings(&mut self) -> Result<Vec<Vec<Coord>>, ()> {
        let mut rings = Vec::new();
        loop {
            self.expect(b'(')?;
            let coords = self.coord_list(4)?;
            let first = coords[0];
            let last = *coords.last().expect("min 4");
            if first.x.to_bits() != last.x.to_bits() || first.y.to_bits() != last.y.to_bits() {
                return Err(());
            }
            self.expect(b')')?;
            rings.push(coords);
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.i += 1;
                continue;
            }
            break;
        }
        if rings.is_empty() {
            return Err(());
        }
        Ok(rings)
    }

    fn coord_list(&mut self, min: usize) -> Result<Vec<Coord>, ()> {
        let mut pts = vec![self.coord()?];
        loop {
            self.skip_ws();
            if self.peek() == Some(b',') {
                self.i += 1;
                pts.push(self.coord()?);
            } else {
                break;
            }
        }
        if pts.len() < min || pts.len() > MAX_COORDS {
            return Err(());
        }
        Ok(pts)
    }

    fn coord(&mut self) -> Result<Coord, ()> {
        let x = self.number()?;
        let y = self.number()?;
        self.skip_ws();
        if let Some(c) = self.peek() {
            if c == b'+' || c == b'-' || c == b'.' || c.is_ascii_digit() {
                return Err(());
            }
        }
        Ok(Coord { x, y })
    }

    fn number(&mut self) -> Result<f64, ()> {
        self.skip_ws();
        let start = self.i;
        if matches!(self.peek(), Some(b'+' | b'-')) {
            self.i += 1;
        }
        let mut saw_digit = false;
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            saw_digit = true;
            self.i += 1;
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                saw_digit = true;
                self.i += 1;
            }
        }
        if !saw_digit {
            return Err(());
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            let mut exp = false;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                exp = true;
                self.i += 1;
            }
            if !exp {
                return Err(());
            }
        }
        let text = std::str::from_utf8(&self.s[start..self.i]).map_err(|_| ())?;
        let n: f64 = text.parse().map_err(|_| ())?;
        if n.is_finite() {
            Ok(n)
        } else {
            Err(())
        }
    }

    fn ident(&mut self) -> Result<String, ()> {
        self.skip_ws();
        let start = self.i;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphabetic()) {
            self.i += 1;
        }
        if start == self.i {
            return Err(());
        }
        Ok(std::str::from_utf8(&self.s[start..self.i])
            .map_err(|_| ())?
            .to_ascii_uppercase())
    }

    fn starts_with_word(&self, word: &str) -> bool {
        let rest = &self.s[self.i..];
        let wb = word.as_bytes();
        if rest.len() < wb.len() {
            return false;
        }
        if !rest[..wb.len()].eq_ignore_ascii_case(wb) {
            return false;
        }
        !rest
            .get(wb.len())
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
    }

    fn expect(&mut self, byte: u8) -> Result<(), ()> {
        self.skip_ws();
        if self.peek() == Some(byte) {
            self.i += 1;
            Ok(())
        } else {
            Err(())
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(wkt: &str) -> String {
        let payload = geom_from_wkt(wkt, 0, None).unwrap();
        geom_as_text(&payload).unwrap()
    }

    #[test]
    fn wkt_matches_mysql_astext() {
        assert_eq!(text("POINT(1 2)"), "POINT(1 2)");
        assert_eq!(text("point(1.0 2.50)"), "POINT(1 2.5)");
        assert_eq!(text("LINESTRING(0 0, 1 1)"), "LINESTRING(0 0,1 1)");
        assert_eq!(
            text("POLYGON((0 0, 1 0, 1 1, 0 0))"),
            "POLYGON((0 0,1 0,1 1,0 0))"
        );
        assert_eq!(
            text("POLYGON((0 0, 10 0, 10 10, 0 10, 0 0),(1 1, 2 1, 2 2, 1 2, 1 1))"),
            "POLYGON((0 0,10 0,10 10,0 10,0 0),(1 1,2 1,2 2,1 2,1 1))"
        );
        assert_eq!(text("MULTIPOINT(1 1, 2 2)"), "MULTIPOINT((1 1),(2 2))");
        assert_eq!(
            text("MULTILINESTRING((0 0, 1 1),(2 2, 3 3))"),
            "MULTILINESTRING((0 0,1 1),(2 2,3 3))"
        );
        assert_eq!(
            text("MULTIPOLYGON(((0 0, 1 0, 1 1, 0 0)))"),
            "MULTIPOLYGON(((0 0,1 0,1 1,0 0)))"
        );
        assert_eq!(
            text("GEOMETRYCOLLECTION(POINT(1 1),LINESTRING(0 0, 1 1))"),
            "GEOMETRYCOLLECTION(POINT(1 1),LINESTRING(0 0,1 1))"
        );
    }

    #[test]
    fn point_wkb_matches_mysql_hex() {
        let payload = geom_from_wkt("POINT(1 1)", 0, None).unwrap();
        assert_eq!(
            payload,
            "000000000101000000000000000000f03f000000000000f03f"
        );
    }

    #[test]
    fn srid_4326_round_trip_and_axis_swap() {
        let g = geom_from_wkt("POINT(1 2)", 4326, None).unwrap();
        assert_eq!(geom_as_text(&g).unwrap(), "POINT(1 2)");
        assert_eq!(geom_srid(&g).unwrap(), 4326);
        assert_eq!(geom_latitude(&g).unwrap(), 1.0);
        assert_eq!(geom_longitude(&g).unwrap(), 2.0);
        let swapped = geom_set_srid(&geom_from_wkt("POINT(1 2)", 0, None).unwrap(), 4326).unwrap();
        assert_eq!(geom_as_text(&swapped).unwrap(), "POINT(2 1)");
        assert!(matches!(
            geom_from_wkt("POINT(200 1)", 4326, None),
            Err(GeometryFault::LatitudeOutOfRange(_))
        ));
        assert!(matches!(
            geom_from_wkt("POINT(1 200)", 4326, None),
            Err(GeometryFault::LongitudeOutOfRange(_))
        ));
        assert!(matches!(
            geom_from_wkt("POINT(1 1)", 999999, None),
            Err(GeometryFault::SrsNotFound(999999))
        ));
    }

    #[test]
    fn column_type_srid_and_store() {
        assert_eq!(
            canonicalize_spatial_type_name("POINT(4326)"),
            "POINT SRID 4326"
        );
        assert_eq!(
            canonicalize_spatial_type_name("GEOMETRYCOLLECTION"),
            "GEOMCOLLECTION"
        );
        let point = geom_from_wkt("POINT(1 2)", 0, None).unwrap();
        let line = geom_from_wkt("LINESTRING(0 0,1 1)", 0, None).unwrap();
        assert!(store_spatial_cell("POINT", &point).is_ok());
        assert!(matches!(
            store_spatial_cell("POINT", &line),
            Err(GeometryFault::TypeMismatch)
        ));
        assert!(matches!(
            store_spatial_cell("POINT", "POINT(1 2)"),
            Err(GeometryFault::NotGeometry)
        ));
        let geo = geom_from_wkt("POINT(1 2)", 4326, None).unwrap();
        assert!(matches!(
            store_spatial_cell("POINT SRID 4326", &point),
            Err(GeometryFault::SridMismatch {
                geometry: 0,
                column_srid: 4326
            })
        ));
        assert!(store_spatial_cell("POINT SRID 4326", &geo).is_ok());
        assert!(matches!(
            validate_spatial_column("POINT SRID 999999"),
            Err(GeometryFault::SrsNotFound(999999))
        ));
    }
}
