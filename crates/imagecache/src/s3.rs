/*
 * SPDX-FileCopyrightText: Copyright (c) 2026 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 * http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use std::path::Path;

use s3::creds::Credentials;
use s3::{Bucket, Region};
use tokio::io::AsyncReadExt;
use tracing::warn;

use crate::config::S3Config;
use crate::error::ImageCacheError;
use crate::storage::StorageBackend;

/// 64 MiB multipart chunk size. The rust-s3 default (8 MiB) creates ~1152
/// concurrent requests for a 9 GiB file, exhausting file descriptors.
/// S3 minimum is 5 MiB; maximum parts per upload is 10,000.
/// 64 MiB keeps a 10 GiB upload under 160 parts.
const MULTIPART_CHUNK_SIZE: usize = 64 * 1024 * 1024;

pub struct S3Client {
    bucket: Box<Bucket>,
}

impl S3Client {
    pub fn new(config: &S3Config) -> Result<Self, ImageCacheError> {
        let region = Region::Custom {
            region: config.region.clone(),
            endpoint: config.endpoint.clone(),
        };
        let credentials = Credentials::new(
            Some(&config.access_key),
            Some(&config.secret_key),
            None,
            None,
            None,
        )
        .map_err(|e| ImageCacheError::S3(format!("Failed to create S3 credentials: {e}")))?;

        let bucket = Bucket::new(&config.bucket, region, credentials)
            .map_err(|e| ImageCacheError::S3(format!("Failed to create S3 bucket handle: {e}")))?
            .with_path_style();

        Ok(Self { bucket })
    }

    async fn upload_and_complete_multipart(
        &self,
        key: &str,
        file_path: &Path,
        upload_id: &str,
        content_type: &str,
    ) -> Result<(), ImageCacheError> {
        let mut file = tokio::fs::File::open(file_path).await?;
        let mut part_number: u32 = 0;
        let mut parts = Vec::new();
        let mut buf = vec![0u8; MULTIPART_CHUNK_SIZE];

        loop {
            let mut bytes_read = 0;
            // Fill the buffer completely (or to EOF)
            while bytes_read < MULTIPART_CHUNK_SIZE {
                match file.read(&mut buf[bytes_read..]).await? {
                    0 => break,
                    n => bytes_read += n,
                }
            }
            if bytes_read == 0 {
                break;
            }

            part_number += 1;
            let chunk = buf[..bytes_read].to_vec();
            let part = self
                .bucket
                .put_multipart_chunk(chunk, key, part_number, upload_id, content_type)
                .await
                .map_err(|e| {
                    ImageCacheError::S3(format!(
                        "Multipart chunk {part_number} failed for key {key}: {e}"
                    ))
                })?;
            parts.push(part);
        }

        let response = self
            .bucket
            .complete_multipart_upload(key, upload_id, parts)
            .await
            .map_err(|e| {
                ImageCacheError::S3(format!("Complete multipart failed for key {key}: {e}"))
            })?;
        let code = response.status_code();
        if !(200..300).contains(&code) {
            return Err(ImageCacheError::S3(format!(
                "Complete multipart returned status {code} for key {key}"
            )));
        }
        Ok(())
    }
}

impl StorageBackend for S3Client {
    async fn object_exists(&self, key: &str) -> Result<bool, ImageCacheError> {
        match self.bucket.head_object(key).await {
            Ok((_, code)) if (200..300).contains(&code) => Ok(true),
            Ok((_, 404)) => Ok(false),
            Ok((_, code)) => Err(ImageCacheError::S3(format!(
                "HEAD request returned unexpected status {code} for key {key}"
            ))),
            Err(e) => Err(ImageCacheError::S3(format!(
                "HEAD request failed for key {key}: {e}"
            ))),
        }
    }

    async fn put_object_from_file(
        &self,
        key: &str,
        file_path: &Path,
    ) -> Result<(), ImageCacheError> {
        let file_size = tokio::fs::metadata(file_path).await?.len() as usize;

        // Small files: single PUT (under the 5 GiB S3 limit)
        if file_size <= MULTIPART_CHUNK_SIZE {
            let data = tokio::fs::read(file_path).await?;
            let response = self
                .bucket
                .put_object(key, &data)
                .await
                .map_err(|e| ImageCacheError::S3(format!("PUT failed for key {key}: {e}")))?;
            let code = response.status_code();
            if !(200..300).contains(&code) {
                return Err(ImageCacheError::S3(format!(
                    "PUT returned status {code} for key {key}"
                )));
            }
            return Ok(());
        }

        // Large files: sequential multipart upload
        let content_type = "application/octet-stream";
        let msg = self
            .bucket
            .initiate_multipart_upload(key, content_type)
            .await
            .map_err(|e| {
                ImageCacheError::S3(format!("Initiate multipart failed for key {key}: {e}"))
            })?;
        let upload_id = &msg.upload_id;

        let result = self
            .upload_and_complete_multipart(key, file_path, upload_id, content_type)
            .await;

        if result.is_err()
            && let Err(abort_err) = self.bucket.abort_upload(key, upload_id).await
        {
            warn!(
                key = key,
                error = %abort_err,
                "Failed to abort multipart upload, orphaned parts may remain"
            );
        }

        result
    }
}
