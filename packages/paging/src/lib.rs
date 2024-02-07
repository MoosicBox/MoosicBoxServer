#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use moosicbox_core::sqlite::models::ToApi;
use std::{ops::Deref, pin::Pin, sync::Arc};

use futures::Future;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum Page<T> {
    WithTotal {
        items: Vec<T>,
        offset: u32,
        limit: u32,
        total: u32,
    },
    WithHasMore {
        items: Vec<T>,
        offset: u32,
        limit: u32,
        has_more: bool,
    },
}

impl<T> Page<T> {
    pub fn offset(&self) -> u32 {
        match self {
            Self::WithTotal { offset, .. } => *offset,
            Self::WithHasMore { offset, .. } => *offset,
        }
    }

    pub fn limit(&self) -> u32 {
        match self {
            Self::WithTotal { limit, .. } => *limit,
            Self::WithHasMore { limit, .. } => *limit,
        }
    }

    pub fn has_more(&self) -> bool {
        match self {
            Self::WithTotal {
                items,
                offset,
                total,
                ..
            } => *offset + (items.len() as u32) < *total,
            Self::WithHasMore { has_more, .. } => *has_more,
        }
    }

    pub fn total(&self) -> Option<u32> {
        match self {
            Self::WithTotal { total, .. } => Some(*total),
            Self::WithHasMore { .. } => None,
        }
    }

    pub fn items(self) -> Vec<T> {
        match self {
            Self::WithTotal { items, .. } => items,
            Self::WithHasMore { items, .. } => items,
        }
    }
}

type FuturePagingResponse<T, E> = Pin<Box<dyn Future<Output = PagingResult<T, E>> + Send>>;
type FetchPagingResponse<T, E> = Box<dyn FnMut(u32, u32) -> FuturePagingResponse<T, E> + Send>;

pub struct PagingResponse<T, E> {
    pub page: Page<T>,
    pub fetch: Arc<Mutex<FetchPagingResponse<T, E>>>,
}

impl<T, E> PagingResponse<T, E> {
    pub async fn rest_of_pages_in_batches(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_in_batches_inner(false).await
    }

    async fn rest_of_pages_in_batches_inner(self, include_self: bool) -> Result<Vec<Page<T>>, E> {
        let total = if let Some(total) = self.total() {
            total
        } else {
            return self.rest_of_pages_inner(include_self).await;
        };

        let limit = self.limit();
        let mut offset = self.offset() + limit;
        let mut requests = vec![];

        while offset < total {
            log::debug!(
                "Adding request into batch: request {} offset={offset} limit={limit}",
                requests.len() + 1
            );
            requests.push((offset, limit));

            offset += limit;
        }

        let mut responses = vec![];

        if include_self {
            responses.push(self.page);
        }

        let mut fetch = self.fetch.lock().await;
        let page_responses = futures::future::join_all(
            requests
                .into_iter()
                .map(|(offset, limit)| fetch(offset, limit)),
        )
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

        for response in page_responses {
            responses.push(response.page);
        }

        Ok(responses)
    }

    pub async fn rest_of_items_in_batches(self) -> Result<Vec<T>, E> {
        Ok(self
            .rest_of_pages_in_batches()
            .await?
            .into_iter()
            .flat_map(|response| response.items())
            .collect::<Vec<_>>())
    }

    pub async fn with_rest_of_pages_in_batches(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_in_batches_inner(true).await
    }

    pub async fn with_rest_of_items_in_batches(self) -> Result<Vec<T>, E> {
        Ok(self
            .with_rest_of_pages_in_batches()
            .await?
            .into_iter()
            .flat_map(|response| response.items())
            .collect::<Vec<_>>())
    }

    pub async fn rest_of_pages(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_inner(false).await
    }

    async fn rest_of_pages_inner(self, include_self: bool) -> Result<Vec<Page<T>>, E> {
        let mut limit = self.limit();
        let mut offset = self.offset() + limit;
        let mut fetch = self.fetch;
        let mut responses = vec![];

        if include_self {
            responses.push(self.page);
        }

        loop {
            let response = (fetch.lock().await)(offset, limit).await?;

            let has_more = response.has_more();
            limit = response.limit();
            offset = response.offset() + limit;
            fetch = response.fetch;

            responses.push(response.page);

            if !has_more {
                break;
            }
        }

        Ok(responses)
    }

    pub async fn rest_of_items(self) -> Result<Vec<T>, E> {
        Ok(self
            .rest_of_pages()
            .await?
            .into_iter()
            .flat_map(|response| response.items())
            .collect::<Vec<_>>())
    }

    pub async fn with_rest_of_pages(self) -> Result<Vec<Page<T>>, E> {
        self.rest_of_pages_inner(true).await
    }

    pub async fn with_rest_of_items(self) -> Result<Vec<T>, E> {
        Ok(self
            .with_rest_of_pages()
            .await?
            .into_iter()
            .flat_map(|response| response.items())
            .collect::<Vec<_>>())
    }

    pub fn offset(&self) -> u32 {
        self.page.offset()
    }

    pub fn limit(&self) -> u32 {
        self.page.limit()
    }

    pub fn has_more(&self) -> bool {
        self.page.has_more()
    }

    pub fn total(&self) -> Option<u32> {
        self.page.total()
    }

    pub fn items(self) -> Vec<T> {
        self.page.items()
    }

    pub fn map<U, F, OE>(self, mut f: F) -> PagingResponse<U, OE>
    where
        F: FnMut(T) -> U + Send + Clone + 'static,
        T: 'static,
        OE: 'static,
        E: Into<OE> + 'static,
    {
        let page = match self.page {
            Page::WithTotal {
                items,
                offset,
                limit,
                total,
            } => Page::WithTotal {
                items: items.into_iter().map(&mut f).collect::<Vec<_>>(),
                offset,
                limit,
                total,
            },
            Page::WithHasMore {
                items,
                offset,
                limit,
                has_more,
            } => Page::WithHasMore {
                items: items.into_iter().map(&mut f).collect::<Vec<_>>(),
                offset,
                limit,
                has_more,
            },
        };

        let fetch = self.fetch;

        PagingResponse {
            page,
            fetch: Arc::new(Mutex::new(Box::new(move |offset, count| {
                let fetch = fetch.clone();
                let f = f.clone();

                let closure = async move {
                    let mut fetch = fetch.lock().await;
                    fetch(offset, count)
                        .await
                        .map_err(|e| e.into())
                        .map(|results| results.map(f))
                };

                Box::pin(closure)
            }))),
        }
    }
}

impl<T, E> Deref for PagingResponse<T, E> {
    type Target = Page<T>;

    fn deref(&self) -> &Self::Target {
        &self.page
    }
}

impl<T> Deref for Page<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::WithTotal { items, .. } => items,
            Self::WithHasMore { items, .. } => items,
        }
    }
}

impl<T, E> From<PagingResponse<T, E>> for Page<T> {
    fn from(value: PagingResponse<T, E>) -> Self {
        value.page
    }
}

impl<T> From<Page<T>> for Vec<T> {
    fn from(value: Page<T>) -> Self {
        match value {
            Page::WithTotal { items, .. } => items,
            Page::WithHasMore { items, .. } => items,
        }
    }
}

impl<In, Out, E> ToApi<PagingResponse<Out, E>> for PagingResponse<In, E>
where
    In: ToApi<Out> + Clone + 'static,
    E: 'static,
{
    fn to_api(self) -> PagingResponse<Out, E> {
        self.map(|item| item.to_api())
    }
}

pub type PagingResult<T, E> = Result<PagingResponse<T, E>, E>;