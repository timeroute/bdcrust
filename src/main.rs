use geo::ChamberlainDuquetteArea;
use geo::Intersects;
use geo::MultiPolygon;
use geojson::Feature;
use geojson::GeoJson;
use geojson::Geometry;
use geojson::JsonObject;
use geojson::JsonValue;
use proj::Transform;
use shapefile::Reader;
use shapefile::{self, dbase::Record};
use std::env;
use std::fs;
use std::io::BufReader;
use std::io::Cursor;
use std::io::Read;
use std::path::Path;
use zip;

struct ShapeRecord {
    polygon: MultiPolygon<f64>,
    record: Record,
}

fn transform(mut polygon: MultiPolygon<f64>, from: &str) -> MultiPolygon {
    polygon.transform_crs_to_crs(from, "EPSG:4326").unwrap();
    polygon
}

fn read_shape_and_record(mut reader: Reader<Cursor<Vec<u8>>>, from: &str) -> Vec<ShapeRecord> {
    let mut datas = vec![];
    for result in reader.iter_shapes_and_records() {
        let (shape, record) = result.unwrap();
        match shape {
            shapefile::Shape::Polygon(shape) => {
                let mut polygon: MultiPolygon<f64> = shape.into();
                polygon = transform(polygon.clone(), from);
                datas.push(ShapeRecord { polygon, record });
            }
            shapefile::Shape::PolygonZ(shape) => {
                let mut polygon: MultiPolygon<f64> = shape.into();
                polygon = transform(polygon.clone(), from);
                datas.push(ShapeRecord { polygon, record });
            }
            _ => {}
        }
    }
    datas
}

fn filter_zip_file(dir_path: &str) -> Vec<String> {
    let mut filter_files = vec![];
    let files = fs::read_dir(dir_path).unwrap();
    for result in files {
        let file = result.unwrap();
        let ori_name = file.file_name();
        let file_name = String::from(ori_name.to_string_lossy());
        if file_name.ends_with(".zip") {
            filter_files.push(file_name);
        }
    }
    filter_files
}

fn read_zip_file<'a>(dir_path: &'a str, fname: &'a str) -> Reader<Cursor<Vec<u8>>> {
    let fpath = Path::new(dir_path).join(fname);
    let zfile = fs::File::open(fpath).unwrap();
    let reader = BufReader::new(zfile);
    let mut archive = zip::ZipArchive::new(reader).unwrap();
    let mut shp_file_name = String::new();
    let mut dbf_file_name = String::new();
    for name in archive.file_names() {
        if name.ends_with(".shp") {
            shp_file_name = name.to_owned();
        }
        if name.ends_with(".dbf") {
            dbf_file_name = name.to_owned();
        }
    }
    let shp_buffer = {
        let mut file = archive.by_name(&shp_file_name).unwrap();
        let mut buffer = vec![];
        file.read_to_end(&mut buffer).unwrap();
        buffer
    };
    let dbf_buffer = {
        let mut file = archive.by_name(&dbf_file_name).unwrap();
        let mut buffer = vec![];
        file.read_to_end(&mut buffer).unwrap();
        buffer
    };
    let shp_reader = shapefile::ShapeReader::new(Cursor::new(shp_buffer)).unwrap();
    let dbf_reader = shapefile::dbase::Reader::new(Cursor::new(dbf_buffer)).unwrap();
    let reader = shapefile::Reader::new(shp_reader, dbf_reader);
    reader
}

fn to_geojson(data: &ShapeRecord, properties: JsonObject) -> GeoJson {
    let geometry = Geometry::from(&data.polygon);
    let geojson = GeoJson::Feature(Feature {
        bbox: None,
        geometry: Some(geometry),
        id: None,
        properties: Some(properties),
        foreign_members: None,
    });
    geojson
}

fn calc(lb_datas: &Vec<ShapeRecord>, qz_datas: &Vec<ShapeRecord>) {
    for qz_data in qz_datas {
        let qz_id = match qz_data.record.get("ZDDM") {
            Some(shapefile::dbase::FieldValue::Character(Some(v))) => v,
            Some(_) => panic!("Expected sub 'id'"),
            None => panic!("Expected sub 'none'"),
        };
        let mut properties = JsonObject::new();
        for lb_data in lb_datas {
            let is_inter = lb_data.polygon.intersects(&qz_data.polygon);
            if is_inter {
                let lb_id = match lb_data.record.get("XBNO") {
                    Some(shapefile::dbase::FieldValue::Character(Some(v))) => v.to_string(),
                    Some(_) => panic!("Expected 'id'"),
                    None => panic!("Expected 'none'"),
                };
                properties.insert(
                    String::from(lb_id),
                    JsonValue::from(lb_data.polygon.chamberlain_duquette_unsigned_area() / 666.66),
                );
            }
        }
        let geojson = to_geojson(&qz_data, properties);
        let mut filename = String::from(qz_id);
        filename.push_str(".json");
        fs::write(filename, geojson.to_string()).unwrap();
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        panic!("输入参数错误，必须带 林班路径 和 林权路径");
    }
    let lb_from = "EPSG:4490";
    let lb_path = &args[1];
    let lb_filter_files = filter_zip_file(lb_path);
    let mut lb_datas = vec![];
    for file in &lb_filter_files {
        let reader = read_zip_file(&lb_path, &file);
        let datas = read_shape_and_record(reader, lb_from);
        lb_datas.push(datas);
    }
    let qz_from = "EPSG:4527";
    let qz_path = &args[2];
    let qz_filter_files = filter_zip_file(qz_path);
    let mut qz_datas = vec![];
    for file in &qz_filter_files {
        let reader = read_zip_file(&qz_path, &file);
        let datas = read_shape_and_record(reader, qz_from);
        qz_datas.push(datas);
    }
    for qz_data in &qz_datas {
        for lb_data in &lb_datas {
            calc(lb_data, qz_data)
        }
    }
}
