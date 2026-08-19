use crate::error::{Result, ShotgunError};
use crate::mapping::types::MappingFile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingFormat {
    Toml,
    Yaml,
    Json,
}

impl MappingFormat {
    pub fn from_path(path: &str) -> MappingFormat {
        if path.ends_with(".yaml") || path.ends_with(".yml") {
            MappingFormat::Yaml
        } else if path.ends_with(".json") {
            MappingFormat::Json
        } else {
            MappingFormat::Toml
        }
    }

    pub fn parse(name: &str) -> MappingFormat {
        match name {
            "yaml" | "yml" => MappingFormat::Yaml,
            "json" => MappingFormat::Json,
            _ => MappingFormat::Toml,
        }
    }
}

pub fn read_mapping_file(path: &str) -> Result<MappingFile> {
    let contents = std::fs::read_to_string(path).map_err(|e| ShotgunError::SpecRead {
        path: path.to_string(),
        source: e,
    })?;
    parse_mapping_str(&contents, MappingFormat::from_path(path)).map_err(|e| {
        ShotgunError::MappingParse {
            path: path.to_string(),
            source: e,
        }
    })
}

pub fn parse_mapping_str(contents: &str, format: MappingFormat) -> anyhow::Result<MappingFile> {
    Ok(match format {
        MappingFormat::Toml => toml::from_str(contents)?,
        MappingFormat::Yaml => serde_yaml::from_str(contents)?,
        MappingFormat::Json => serde_json::from_str(contents)?,
    })
}
