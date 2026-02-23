// Copyright (c) 2026 Unfolded Circle ApS, Markus Zehnder <markus.z@unfoldedcircle.com>
// SPDX-License-Identifier: MPL-2.0

//! Home Assistant media player browsing and search implementation.

use crate::client::messages::{BrowseMedia, SearchMedia};
use crate::client::model::{
    HaBrowseMediaMsg, HaBrowseMediaResponse, HaBrowseMediaResult, HaSearchMediaMsg,
    HaSearchMediaResponse, OpenRequest, transform_media_error,
};
use crate::client::{HomeAssistantClient, set_album_art_proxy};
use crate::errors::ServiceError;
use crate::util::return_fut_err;
use actix::{Handler, ResponseFuture, fut};
use tokio::sync::oneshot;
use uc_api::BrowseMediaItem;
use uc_api::intg::ws::{BrowseMediaResponseMsgData, SearchMediaResponseMsgData};
use uc_api::model::{Pagination, Paging};
use url::Url;

impl Handler<BrowseMedia> for HomeAssistantClient {
    type Result = ResponseFuture<Result<BrowseMediaResponseMsgData, ServiceError>>;

    fn handle(&mut self, msg: BrowseMedia, ctx: &mut Self::Context) -> Self::Result {
        let msg = msg.0;
        let id = self.new_msg_id();
        let (tx, rx) = oneshot::channel();

        // https://developers.home-assistant.io/docs/core/entity/media-player/#browse-media
        let req = HaBrowseMediaMsg {
            id,
            msg_type: "media_player/browse_media".to_string(),
            entity_id: msg.entity_id,
            media_content_id: msg.media_id,
            media_content_type: msg.media_type.map(|v| v.as_str().to_string()),
        };

        if let Err(e) = self.send_json(serde_json::to_value(req).expect("Invalid struct"), ctx) {
            return_fut_err!(e);
        }
        self.open_requests.insert(id, OpenRequest::new(tx));

        let server = self.server.clone();
        Box::pin(async move {
            let resp = rx
                .await
                .map_err(|_| ServiceError::InternalServerError("Channel closed".into()))?;

            let ha_resp: HaBrowseMediaResponse = serde_json::from_value(resp.msg)?;
            transform_browse_response(server, ha_resp, msg.paging.unwrap_or_default())
        })
    }
}

impl Handler<SearchMedia> for HomeAssistantClient {
    type Result = ResponseFuture<Result<SearchMediaResponseMsgData, ServiceError>>;

    fn handle(&mut self, msg: SearchMedia, ctx: &mut Self::Context) -> Self::Result {
        let msg = msg.0;
        let id = self.new_msg_id();
        let (tx, rx) = oneshot::channel();

        // https://developers.home-assistant.io/docs/core/entity/media-player/#search-media
        let req = HaSearchMediaMsg {
            id,
            msg_type: "media_player/search_media".to_string(),
            entity_id: msg.entity_id,
            search_query: msg.query,
            media_content_id: msg.media_id,
            media_content_type: msg.media_type.map(|v| v.as_str().to_string()),
            media_filter_classes: msg.filter.and_then(|f| f.media_classes),
        };

        if let Err(e) = self.send_json(serde_json::to_value(req).expect("Invalid struct"), ctx) {
            return_fut_err!(e);
        }
        self.open_requests.insert(id, OpenRequest::new(tx));

        let server = self.server.clone();
        Box::pin(async move {
            let resp = rx
                .await
                .map_err(|_| ServiceError::InternalServerError("Channel closed".into()))?;

            let ha_resp: HaSearchMediaResponse = serde_json::from_value(resp.msg)?;
            transform_search_response(server, ha_resp, msg.paging.unwrap_or_default())
        })
    }
}

fn transform_browse_response(
    server: Url,
    response: HaBrowseMediaResponse,
    paging: Paging,
) -> Result<BrowseMediaResponseMsgData, ServiceError> {
    if response.success
        && let Some(result) = response.result
    {
        Ok(map_ha_browse(server, result, paging))
    } else if let Some(error) = response.error {
        Err(transform_media_error(error))
    } else {
        Err(ServiceError::InternalServerError(format!(
            "HA returned error: {:?}",
            response.result
        )))
    }
}

fn map_ha_browse(
    server: Url,
    mut ha_resp: HaBrowseMediaResult,
    paging: Paging,
) -> BrowseMediaResponseMsgData {
    let mut total = 0;

    let mut items = Vec::with_capacity(if ha_resp.children.is_some() {
        paging.limit() as usize
    } else {
        0
    });

    if let Some(children) = ha_resp.children.take() {
        total = children.len() as u32;

        items.extend(
            children
                .into_iter()
                .skip(paging.offset() as usize)
                .take(paging.limit() as usize)
                .map(|c| c.into())
                .map(|mut item: BrowseMediaItem| {
                    if let Some(thumbnail) = item.thumbnail.as_deref() {
                        if let Some(url) = set_album_art_proxy(&server, thumbnail) {
                            item.thumbnail = Some(url);
                        }
                    }
                    item
                }),
        );
    }

    let mut root: BrowseMediaItem = ha_resp.into();
    if let Some(thumbnail) = root.thumbnail.as_deref() {
        if let Some(url) = set_album_art_proxy(&server, thumbnail) {
            root.thumbnail = Some(url);
        }
    }
    if !items.is_empty() {
        root.items = Some(items);
    }

    BrowseMediaResponseMsgData {
        media: Some(root),
        pagination: Pagination::new(total, paging.limit(), paging.page()),
    }
}

fn transform_search_response(
    server: Url,
    response: HaSearchMediaResponse,
    paging: Paging,
) -> Result<SearchMediaResponseMsgData, ServiceError> {
    if response.success
        && let Some(result) = response.result
    {
        Ok(map_ha_search(
            server,
            result.result.unwrap_or_default(),
            paging,
        ))
    } else if let Some(error) = response.error {
        Err(transform_media_error(error))
    } else {
        Err(ServiceError::InternalServerError(format!(
            "HA returned error: {:?}",
            response.result
        )))
    }
}

fn map_ha_search(
    server: Url,
    ha_resp: Vec<HaBrowseMediaResult>,
    paging: Paging,
) -> SearchMediaResponseMsgData {
    let total = ha_resp.len() as u32;

    let items = ha_resp
        .into_iter()
        .skip(paging.offset() as usize)
        .take(paging.limit() as usize)
        .map(|c| c.into())
        .map(|mut item: BrowseMediaItem| {
            if let Some(thumbnail) = item.thumbnail.as_deref() {
                if let Some(url) = set_album_art_proxy(&server, thumbnail) {
                    item.thumbnail = Some(url);
                }
            }
            item
        })
        .collect();

    SearchMediaResponseMsgData {
        media: items,
        pagination: Pagination::new(total, paging.limit(), paging.page()),
    }
}
