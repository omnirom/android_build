use anyhow::Result;
use serde::{Deserialize, Deserializer};
use serde_xml_rs::{from_reader, from_str};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct Api {
    #[serde(rename = "version")]
    pub version: String,
    #[serde(rename = "sdk")]
    pub sdks: Vec<Sdk>,
    #[serde(rename = "class", deserialize_with = "to_map_using_name")]
    pub classes: BTreeMap<String, Class>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct Class {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "since")]
    pub since: String,
    #[serde(rename = "deprecated")]
    pub deprecated: Option<String>,
    #[serde(rename = "sdks")]
    pub sdks: Option<String>,
    #[serde(rename = "module")]
    pub modelue_text: Option<String>,
    #[serde(default)]
    pub extends: Vec<Extends>,
    #[serde(rename = "method", deserialize_with = "to_map_using_name", default)]
    pub methods: BTreeMap<String, Method>,

    #[serde(rename = "field", deserialize_with = "to_map_using_name", default)]
    pub fields: BTreeMap<String, Field>,
    #[serde(default)]
    pub implements: Vec<Implements>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct Method {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "since")]
    pub since: Option<String>,
    #[serde(rename = "deprecated")]
    pub deprecated: Option<String>,
    #[serde(rename = "sdks")]
    pub sdks: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct Sdk {
    #[serde(rename = "id")]
    pub id: String,
    #[serde(rename = "shortname")]
    pub shortname: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "reference")]
    pub reference: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct Extends {
    #[serde(rename = "name")]
    pub name: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct Field {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "since")]
    pub since: Option<String>,
    #[serde(rename = "deprecated")]
    pub deprecated: Option<String>,
    #[serde(rename = "sdks")]
    pub sdks: Option<String>,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
pub struct Implements {
    #[serde(rename = "name")]
    pub name: String,
}

trait Named {
    fn get_name(&self) -> String;
}

macro_rules! impl_Named {
    ($($t:ty),+) => {
        $(impl Named for $t {
            fn get_name(&self) -> String {
                self.name.clone()
            }
        })*
    }
}

impl_Named!(Class, Field, Method);

fn to_map_using_name<'de, D, T>(deserializer: D) -> Result<BTreeMap<String, T>, D::Error>
where
    D: Deserializer<'de>,
    T: Named + Deserialize<'de>,
{
    let as_vec: Vec<T> = Deserialize::deserialize(deserializer)?;
    let as_map = as_vec.into_iter().map(|x| (x.get_name(), x)).collect::<BTreeMap<_, _>>();
    Ok(as_map)
}

/// Load an api file
pub fn load<P: AsRef<Path>>(filename: P) -> Result<Api> {
    let file = File::open(filename)?;
    let file = BufReader::new(file);
    let api: Api = from_reader(file)?;
    Ok(api)
}

#[allow(dead_code)]
pub fn parse_class(xml: &str) -> Result<Class> {
    let class: Class = from_str(xml)?;
    Ok(class)
}
