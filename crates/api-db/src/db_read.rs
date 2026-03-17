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

use futures_util::future::BoxFuture;
use futures_util::stream::BoxStream;
use sqlx::{Database, Describe, Either, Execute, PgConnection, PgPool, PgTransaction, Postgres};

/// Database handle used for read-only operations.
///
/// A DbReader can be converted from a PgPool, a borrowed &PgPool, a &mut PgConnection, or a &mut
/// PgTransaction, each via the `.as_db_reader()` method from the [`AsDbReader`] trait. Converting
/// to a DbReader is cheap and involves no allocations.
#[derive(Debug)]
pub enum DbReader<'a> {
    Pool(PgPool),
    BorrowedPool(&'a PgPool),
    PgConnection(&'a mut PgConnection),
}

impl<'a> From<PgPool> for DbReader<'a> {
    fn from(pool: PgPool) -> Self {
        Self::Pool(pool)
    }
}

impl<'a> From<&'a PgPool> for DbReader<'a> {
    fn from(pool: &'a PgPool) -> Self {
        Self::BorrowedPool(pool)
    }
}

impl<'a> From<&'a mut PgConnection> for DbReader<'a> {
    fn from(conn: &'a mut PgConnection) -> Self {
        Self::PgConnection(conn)
    }
}

impl<'a, 'txn> From<&'a mut PgTransaction<'txn>> for DbReader<'a> {
    fn from(txn: &'a mut PgTransaction<'txn>) -> Self {
        Self::PgConnection(txn.as_mut())
    }
}

impl<'a> From<&'a mut crate::Transaction<'a>> for DbReader<'a> {
    fn from(txn: &'a mut crate::Transaction<'a>) -> Self {
        Self::PgConnection(txn.as_pgconn())
    }
}

pub trait AsDbReader {
    fn as_db_reader(&mut self) -> DbReader<'_>;
}

impl AsDbReader for PgConnection {
    fn as_db_reader(&mut self) -> DbReader<'_> {
        DbReader::PgConnection(self)
    }
}

impl AsDbReader for PgTransaction<'_> {
    fn as_db_reader(&mut self) -> DbReader<'_> {
        DbReader::PgConnection(self)
    }
}

impl AsDbReader for crate::Transaction<'_> {
    fn as_db_reader(&mut self) -> DbReader<'_> {
        DbReader::PgConnection(self.as_pgconn())
    }
}

impl AsDbReader for PgPool {
    fn as_db_reader(&mut self) -> DbReader<'_> {
        DbReader::BorrowedPool(self)
    }
}

impl<'c> sqlx::Executor<'c> for &'c mut DbReader<'_> {
    type Database = Postgres;

    fn fetch_many<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxStream<
        'e,
        Result<
            Either<<Self::Database as Database>::QueryResult, <Self::Database as Database>::Row>,
            sqlx::Error,
        >,
    >
    where
        'c: 'e,
        E: 'q + Execute<'q, Self::Database>,
    {
        match self {
            DbReader::Pool(p) => p.fetch_many(query),
            DbReader::BorrowedPool(p) => p.fetch_many(query),
            DbReader::PgConnection(c) => c.fetch_many(query),
        }
    }

    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<<Self::Database as Database>::Row>, sqlx::Error>>
    where
        'c: 'e,
        E: 'q + Execute<'q, Self::Database>,
    {
        match self {
            DbReader::Pool(p) => p.fetch_optional(query),
            DbReader::BorrowedPool(p) => p.fetch_optional(query),
            DbReader::PgConnection(c) => c.fetch_optional(query),
        }
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as Database>::Statement<'q>, sqlx::Error>>
    where
        'c: 'e,
    {
        match self {
            DbReader::Pool(p) => p.prepare_with(sql, parameters),
            DbReader::BorrowedPool(p) => p.prepare_with(sql, parameters),
            DbReader::PgConnection(c) => c.prepare_with(sql, parameters),
        }
    }

    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> BoxFuture<'e, Result<Describe<Self::Database>, sqlx::Error>>
    where
        'c: 'e,
    {
        match self {
            DbReader::Pool(p) => p.describe(sql),
            DbReader::BorrowedPool(p) => p.describe(sql),
            DbReader::PgConnection(c) => c.describe(sql),
        }
    }
}

impl<'c, 'txn> sqlx::Executor<'c> for &'c mut crate::Transaction<'txn> {
    type Database = Postgres;

    fn fetch_many<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxStream<
        'e,
        Result<
            Either<<Self::Database as Database>::QueryResult, <Self::Database as Database>::Row>,
            sqlx::Error,
        >,
    >
    where
        'c: 'e,
        E: 'q + Execute<'q, Self::Database>,
    {
        self.inner.fetch_many(query)
    }

    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<<Self::Database as Database>::Row>, sqlx::Error>>
    where
        'c: 'e,
        E: 'q + Execute<'q, Self::Database>,
    {
        self.inner.fetch_optional(query)
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as Database>::Statement<'q>, sqlx::Error>>
    where
        'c: 'e,
    {
        self.inner.prepare_with(sql, parameters)
    }

    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> BoxFuture<'e, Result<Describe<Self::Database>, sqlx::Error>>
    where
        'c: 'e,
    {
        self.inner.describe(sql)
    }
}
