// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

use crate::display::{HeadUnit, Record, Screen, Size};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

pub const ASSET: &str = include_str!("../../../android/app/src/main/assets/car-models.json");
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CarSelection {
    pub brand: String,
    pub brand_text: String,
    pub model: String,
    pub model_text: String,
    pub year: String,
}
#[derive(Deserialize)]
pub struct CarModels {
    pub format: u8,
    pub brands: Vec<Brand>,
}
#[derive(Deserialize)]
pub struct Brand {
    pub id: String,
    pub names: BTreeMap<String, String>,
    pub models: Vec<Model>,
}
#[derive(Deserialize)]
pub struct Model {
    pub id: String,
    pub names: BTreeMap<String, String>,
    pub years: Vec<Year>,
}
#[derive(Deserialize)]
pub struct Year {
    pub id: String,
    pub names: BTreeMap<String, String>,
    pub screens: Vec<ScreenData>,
}
#[derive(Deserialize)]
pub struct ScreenData {
    pub screen: Screen,
    pub fullscreen: Vec<ScreenSize>,
    pub windowed: Vec<ScreenSize>,
}
#[derive(Deserialize)]
pub struct ScreenSize {
    pub width: u32,
    pub height: u32,
    pub count: u64,
}
impl CarModels {
    pub fn parse(text: &str) -> crate::Result<Self> {
        let data: Self = serde_json::from_str(text)?;
        let names =
            |n: &BTreeMap<String, String>| n.get("en").is_some_and(|s| !s.trim().is_empty());
        let valid = data.format == 2
            && data.brands.iter().all(|b| {
                names(&b.names)
                    && b.models.iter().all(|m| {
                        names(&m.names)
                            && m.years.iter().all(|y| {
                                names(&y.names)
                                    && y.screens.iter().all(|s| {
                                        s.screen.valid()
                                            && s.fullscreen.iter().chain(&s.windowed).all(|r| {
                                                Size {
                                                    width: r.width,
                                                    height: r.height,
                                                }
                                                .valid()
                                                    && r.count > 0
                                            })
                                    })
                            })
                    })
            });
        if !valid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid car model asset",
            ));
        }
        Ok(data)
    }
    pub fn embedded() -> &'static Self {
        static DATA: OnceLock<CarModels> = OnceLock::new();
        DATA.get_or_init(|| Self::parse(ASSET).expect("valid embedded car models"))
    }
    pub fn year(&self, c: &CarSelection) -> Option<&Year> {
        self.brands
            .iter()
            .find(|b| b.id == c.brand)?
            .models
            .iter()
            .find(|m| m.id == c.model)?
            .years
            .iter()
            .find(|y| y.id == c.year)
    }
}
impl CarSelection {
    pub fn normalized(&self) -> Self {
        let data = CarModels::embedded();
        let brand = data.brands.iter().find(|b| b.id == self.brand);
        let mut c = Self::default();
        if self.brand == "other" {
            c.brand = "other".into();
            c.model = "other".into();
            c.year = "other".into();
        } else if let Some(b) = brand {
            c.brand = b.id.clone();
            let legacy = b
                .models
                .iter()
                .find(|m| self.year.is_empty() && m.years.iter().any(|y| y.id == self.model));
            let model = legacy.or_else(|| b.models.iter().find(|m| m.id == self.model));
            if self.model == "other" {
                c.model = "other".into();
                c.year = "other".into();
            } else if let Some(m) = model {
                c.model = m.id.clone();
                if legacy.is_some() {
                    c.year = self.model.clone();
                } else if self.year == "other" || m.years.iter().any(|y| y.id == self.year) {
                    c.year = self.year.clone();
                }
            }
        }
        let clean = |s: &str| {
            s.chars()
                .filter(|c| !c.is_control())
                .take(40)
                .collect::<String>()
                .trim()
                .to_owned()
        };
        if c.brand == "other" {
            c.brand_text = clean(&self.brand_text);
        }
        if c.model == "other" {
            c.model_text = clean(&self.model_text);
        }
        c
    }
    pub fn key(&self) -> String {
        let c = self.normalized();
        if c == Self::default() {
            String::new()
        } else {
            serde_json::to_string(&c).unwrap()
        }
    }
    pub fn built_in(&self) -> Option<(HeadUnit, Vec<Record>)> {
        let data = CarModels::embedded().year(self)?.screens.first()?;
        let records = [(true, &data.fullscreen), (false, &data.windowed)]
            .into_iter()
            .flat_map(|(fullscreen, rows)| {
                rows.iter().map(move |r| Record {
                    screen: data.screen,
                    size: Size {
                        width: r.width,
                        height: r.height,
                    },
                    fullscreen,
                    count: r.count,
                    last_seen: 0,
                })
            })
            .collect();
        Some((
            HeadUnit {
                screen: data.screen,
                viewport: None,
                fullscreen: Some(true),
                fullscreen_preferred: Some(true),
            },
            records,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn embedded_asset_format_names_and_fallback() {
        let data = CarModels::parse(ASSET).unwrap();
        assert_eq!(data.brands.len(), 2);
        let c = CarSelection {
            brand: "xpeng".into(),
            model: "g6-2023".into(),
            ..Default::default()
        }
        .normalized();
        assert_eq!((&*c.model, &*c.year), ("g6", "g6-2023"));
        let (head, records) = c.built_in().unwrap();
        assert_eq!(head.screen.width, 2401);
        assert!(!records.is_empty());
        assert!(CarSelection {
            year: "g6-2024".into(),
            ..c.clone()
        }
        .built_in()
        .is_none());
        assert!(CarModels::parse(&ASSET.replacen("\"format\": 2", "\"format\": 1", 1)).is_err());
        assert_eq!(
            CarSelection {
                brand: "unknown".into(),
                ..c
            }
            .normalized(),
            CarSelection::default()
        );
    }
    #[test]
    fn other_text_is_clean_and_keys_are_per_car() {
        let c = CarSelection {
            brand: "other".into(),
            brand_text: " A\n ".into(),
            model_text: "B".repeat(50),
            ..Default::default()
        }
        .normalized();
        assert_eq!(c.brand_text, "A");
        assert_eq!(c.model_text.len(), 40);
        assert_eq!(c.year, "other");
        assert_ne!(c.key(), CarSelection::default().key());
    }
}
