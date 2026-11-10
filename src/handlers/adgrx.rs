// SPDX-FileCopyrightText: 2026 Daniel Vrátil <me@dvratil.cz>
//
// SPDX-License-Identifier: MIT

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{Json, State};
use axum::http::StatusCode;
use rand::distr::{Distribution, slice::Choose};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::AppState;

pub async fn impressions_title_handler(
    State(state): State<Arc<AppState<'_>>>,
) -> Result<Json<Value>, StatusCode> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Epoch must be valid");
    let expire = now + Duration::from_hours(1);
    let picture = match state.pictures.get_random_picture().await {
        Ok(Some(picture)) => picture,
        Ok(None) => return Err(StatusCode::NO_CONTENT),
        Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
    };
    let image_url = format!("{}/images/{}", state.config.base_url, picture.id);

    let adid = format!(
        "{}-{}-{}-{}",
        rand_adid(6),
        rand_adid(6),
        rand_adid(6),
        rand_adid(3)
    );

    let guid = format!("{}-1", Uuid::new_v4());

    let response = json!({
        "immersive_banner": [
            {
                "ad": {
                    "adinfo": {
                        "adid": adid,
                        "adtype": "6",
                        "guid": guid,
                        "adResponseVersion": "1.0",
                        "responseTime": now.as_secs().to_string(),
                        "expireTime": expire.as_secs().to_string(),
                        "rotationInterval": "60",
                        "refreshInterval": "50"
                    },
                    "creative": {
                        "cid": "1222896",
                        "clicktype": "6",
                        "clickaction": {
                            "appid": "0",
                            "runtype": "GER",
                            "appcontentid": "",
                            "category": "0000008926"
                        },
                        "adtext": picture.name,
                        "colorPreset": {
                            "buttonBackground": "CP_Plane110",
                            "buttonLabel": "CP_Info1100"
                        },
                        "backgroundimage": {
                            "width": "1920",
                            "height": "1080",
                            "imageurl": image_url,
                            "position": "0"
                        },
                        /*
                        "autoplay": {
                            "video": {
                                "vast": "https:\\/\\/rtb.adgrx.com\\/loads\\/6hq-aTIRt5KhZKCWChCldCRd6wBKNKZR2qkOwpOeEgExGAYgVigBMPPDKTj49ydABkiY8gFgAWgHeALAAQHIAYID0gELQURHMTAzNDdTQU3aAQMxMDPlAYOlUW3oAQDwAQCKAhFjb20uc2Ftc3VuZ3R2LmFkc9ICOHJ0Yi1iaWRkZXItcHJvZC1ldXcxLWRzcC1wcm9kLWV1LXdlc3QtMWEtOWQ5ZGZkOTc4LThxdzh62gIoCAMSJGQ3ODVlMmRiLWQ2ODctMzkzNy1hNjk3LTIwYjUxMzhkYzI1OOgC4cGy1waIAwGQAwKYA2eoAwG4A_HOBsAD5g_KAwVOSUtFTOIDCTIyLjYuMzEyOKIEAkNaqgQEdGlsZbIEDDIyX05JS0VMX1VIRLoEE1QtSU5GT0xJTksyMDIyLTEwMjXCBAEg-AQAkAUCmAUAsgUDMjYwugUCODbABQHIBQLaBXNNb3ppbGxhLzUuMCAoU01BUlQtVFY7IExpbnV4OyBUaXplbiA2LjUpIEFwcGxlV2ViS2l0LzUzOC4xIChLSFRNTCwgbGlrZSBHZWNrbykgVmVyc2lvbi82LjUgTmF0aXZlVFZBZHMgU2FmYXJpLzUzOC4x4gURVC1OS0xCREVVQy0xNzIwLjfoBQLxBc3MzMzMzBJAigYNUUU2NVE2MEJBVVhYSA==\\/654328.xml?AG_WP=0.014003&AG_IT=0&AG_VV=3.0&AG_TID=1222896&AG_APP=3202203026693&IMVID=1&tile_pos=1",
                                "videourl": "https://d3188bz1xcsvdh.cloudfront.net/trader/eb185c0f-44c4-45e1-9a75-31b841525d06/transcoded_1920x1080_Samsung_TV_16x9_CZ_1920x1080MP4.mp4"
                            }
                        },
                        */
                        // We can't keep those entirely empty, otherwise the ad doesn't display
                        "tracking": {
                            "impressionreportingurl": "https://adgrx.com/tracking/impressionreportingurl",
                            "clickreporturl": "https://adgrx.com/tracking/clickreporturl",
                            "thirdpartyclickreporturl": [],
                            "thirdpartyimpressionreportingurl": [],
                            "exitdelayreporturl": "https://adgrx.com/tracking/exitdelayreporturl",
                            "blankreportingurl": "https://adgrx.com/tracking/blankreportingurl",
                            "replayreporturl": "https://adgrx.com/tracking/replayreporturl",
                            "hoverreporturl": "https://adgrx.com/tracking/hoverreporturl",
                            "thirdpartyhoverreporturl": [],
                            "expansiontrackingurl": "https://adgrx.com/tracking/expansiontrackingurl",
                        },
                    }
                }
            }
        ]
    });

    Ok(Json(response))
}

pub async fn blackhole_handler() -> Json<Value> {
    Json(Value::Object(serde_json::Map::default()))
}

fn rand_adid(len: usize) -> String {
    Choose::new(&["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"])
        .expect("Valid alphabet")
        .sample_iter(rand::rng())
        .take(len)
        .copied()
        .collect()
}
