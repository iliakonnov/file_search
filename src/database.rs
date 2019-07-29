use crate::model::{FileInfo, SubvolumeInfo};
use rusqlite::types::{FromSql, FromSqlError, ToSqlOutput, ValueRef};
use rusqlite::{named_params, Error, OptionalExtension, ToSql};

struct Database {
    connection: rusqlite::Connection,
}

struct U64Wrapper(u64);

pub enum AffectedMacros {
    Edited {
        file_id: i64,
        macro_id: i64,
        info: FileInfo,
    },
    New {
        file_id: i64,
        info: FileInfo,
    },
}

impl ToSql for U64Wrapper {
    #[allow(clippy::cast_possible_wrap)]
    fn to_sql(&self) -> Result<ToSqlOutput, Error> {
        let num = self.0 as i64;
        Ok(ToSqlOutput::from(num))
    }
}

impl FromSql for U64Wrapper {
    #[allow(clippy::cast_sign_loss)]
    fn column_result(value: ValueRef) -> Result<Self, FromSqlError> {
        let num: i64 = value.as_i64()?;
        Ok(Self(num as u64))
    }
}

impl Database {
    pub fn connect(path: String) -> Result<Self, Error> {
        let connection = rusqlite::Connection::open(path)?;
        connection.pragma_update(None, "", &"")?;
        Ok(Self { connection })
    }

    pub fn initialize(&mut self) -> Result<(), Error> {
        //noinspection SqlNoDataSourceInspection
        const INIT_SQL: &str = r#"
BEGIN TRANSACTION;

CREATE TABLE "files" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
    "fts_id" INTEGER NOT NULL UNIQUE,
    "path" BLOB NOT NULL,
    "depth" INTEGER NOT NULL,

    "mode" INTEGER NOT NULL,

    "uid" INTEGER NOT NULL,
    "gid" INTEGER NOT NULL,
    
    "atime" INTEGER NOT NULL,
    "mtime" INTEGER NOT NULL,
    "ctime" INTEGER NOT NULL,
    
    "type" INTEGER NOT NULL,
    "length" INTEGER NOT NULL
);

CREATE INDEX "idx_files_ftsid" ON "files" ("fts_id");
CREATE INDEX "idx_files_path" ON "files" ("path");
CREATE INDEX "idx_files_mode" ON "files" ("mode");
CREATE INDEX "idx_files_uid" ON "files" ("uid");
CREATE INDEX "idx_files_gid" ON "files" ("gid");
CREATE INDEX "idx_files_type" ON "files" ("type");
CREATE INDEX "idx_files_length" ON "files" ("length");
CREATE INDEX "idx_files_atime" ON "files" ("atime");
CREATE INDEX "idx_files_mtime" ON "files" ("mtime");
CREATE INDEX "idx_files_ctime" ON "files" ("ctime");

CREATE TABLE "compiled" (
	"macro" INTEGER NOT NULL,
	"file" INTEGER NOT NULL,
	PRIMARY KEY ("macro", "file"),
	FOREIGN KEY ("macro") REFERENCES "macroses"("id") ON DELETE CASCADE,
	FOREIGN KEY ("file") REFERENCES "files"("id") ON DELETE CASCADE
);

CREATE INDEX "idx_compiled_macro" ON "compiled" ("macro");
CREATE INDEX "idx_compiled_version" ON "compiled" ("version");

CREATE TABLE "macroses" (
	"id"	INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
	"query"	TEXT NOT NULL ON DELETE CASCADE
);

CREATE INDEX "idx_macroses_query" ON "macroses" ("query");

CREATE TABLE "volumes" (
	"id"	INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT,
	"type"	INTEGER NOT NULL,
	"data"	TEXT,
	"settings"	TEXT
);

CREATE TABLE "settings" (
    "version" INTEGER NOT NULL,
    "cache_size" INTEGER NOT NULL
);

CREATE TABLE "filters" (
    "id" INTEGER NOT NULL PRIMARY KEY AUTOINCREMENT ,
    "query" TEXT NOT NULL
);
CREATE INDEX "idx_filters_query" ON "filters" ("query");

-- No any separators
CREATE VIRTUAL TABLE files_fts USING fts5(
	path,
	rev_path,
    tokenize = "unicode61 remove_diacritics 0 categories 'L* M* N* P* S* Z* C*'"
);

INSERT INTO "settings" VALUES (
    0,
    10000
);

COMMIT TRANSACTION;
        "#;
        self.connection.execute_batch(INIT_SQL)?;
        Ok(())
    }

    //noinspection SqlNoDataSourceInspection
    pub fn insert_data(
        &mut self,
        subvolumes: Vec<SubvolumeInfo>,
    ) -> Result<Vec<AffectedMacros>, Error> {
        const INSERT_FTS_SQL: &str = r#"
            INSERT INTO "files_fts" ("path", "path_rev")
            VALUES (:path, :path_rev)
        "#;
        const SELECT_FILES_SQL: &str = r#"
            SELECT ("id", "fts_id")
            FROM "files"
            WHERE "path" = :path
        "#;
        const INSERT_FILES_SQL: &str = r#"
            INSERT INTO "files" (
                "fts_id",
                "path",
                "depth",
                "mode",
                "uid",
                "gid",
                "atime",
                "mtime",
                "ctime",
                "type",
                "length"
            )
            VALUES (
                :fts_id,
                :path,
                :depth,
                :mode,
                :uid,
                :gid,
                :atime,
                :mtime,
                :ctime,
                :type,
                :length
            )
        "#;

        // Skips "path" and "depth", because they are not changed
        const UPDATE_FILES_SQL: &str = r#"
            UPDATE "files"
            SET "fts_id" = :fts_id,
                "mode" = :mode,
                "uid" = :uid,
                "gid" = :gid,
                "atime" = :atime,
                "mtime" = :mtime,
                "ctime" = :ctime,
                "type" = :type,
                "length" = :length
            WHERE id = :id
        "#;
        const REMOVE_FTS_SQL: &str = r#"
            DELETE FROM "files_fts"
            WHERE "rowid" = :rowid
        "#;
        const REMOVE_FILES_SQL: &str = r#"
            DELETE FROM "files"
            WHERE "id" = :id
        "#;
        const FIND_MACRO_SQL: &str = r#"
            SELECT DISTINCT "macro" FROM "compiled"
            WHERE "file" = :file
        "#;
        const REMOVE_MACRO_SQL: &str = r#"
            DELETE FROM "macro"
            WHERE "file" = :file
        "#;

        let transaction = self.connection.transaction()?;
        let mut reindex: Vec<AffectedMacros> = Vec::new();

        {
            let mut insert_fts = transaction.prepare_cached(INSERT_FTS_SQL)?;
            let mut insert_files = transaction.prepare_cached(INSERT_FILES_SQL)?;
            let mut select_files = transaction.prepare_cached(SELECT_FILES_SQL)?;
            let mut update_files = transaction.prepare_cached(UPDATE_FILES_SQL)?;
            let mut delete_fts = transaction.prepare_cached(REMOVE_FILES_SQL)?;
            let mut delete_files = transaction.prepare_cached(REMOVE_FTS_SQL)?;
            let mut find_macro = transaction.prepare_cached(FIND_MACRO_SQL)?;
            let mut delete_macro = transaction.prepare_cached(REMOVE_MACRO_SQL)?;

            for subvol in subvolumes {
                for (mut path, file) in subvol.files {
                    let id: Option<(i64, i64)> = select_files
                        .query_row_named(
                            named_params! {
                                ":path": path.to_bytes()
                            },
                            |x| Ok((x.get(0)?, x.get(1)?)),
                        )
                        .optional()?;
                    match file {
                        None => {
                            if let Some((file_id, fts_id)) = id {
                                delete_files.execute_named(named_params! {
                                    ":id": file_id
                                })?;
                                delete_fts.execute_named(named_params! {
                                    ":rowid": fts_id
                                })?;
                                delete_macro.execute_named(named_params! {
                                    ":file": file_id
                                })?;
                            } else {
                                // Do not delete row if it does not exists
                            }
                        }
                        Some(info) => {
                            if let Some((file_id, _fts_id)) = id {
                                update_files.execute_named(named_params! {
                                    ":mode": U64Wrapper(info.permissions),
                                    ":uid": U64Wrapper(info.user_id),
                                    ":gid": U64Wrapper(info.group_id),
                                    ":atime": info.accessed.timestamp_nanos(),
                                    ":mtime": info.modified.timestamp_nanos(),
                                    ":ctime": info.created.timestamp_nanos(),
                                    ":type": info.filetype.to_num(),
                                    ":length": U64Wrapper(info.length)
                                })?;

                                let affected_macroses = find_macro.query_map_named(
                                    named_params! {
                                        ":file": file_id,
                                    },
                                    |row| row.get(0),
                                )?;
                                for macros in affected_macroses {
                                    reindex.push(AffectedMacros::Edited {
                                        file_id,
                                        info: info.clone(),
                                        macro_id: macros?,
                                    });
                                }
                            } else {
                                let path_str = path.to_string();
                                path.reverse();
                                let rev = path.to_string();
                                insert_fts.execute_named(named_params! {
                                    ":path": path_str,
                                    ":path_rev": rev
                                })?;
                                let rowid = transaction.last_insert_rowid();
                                let depth = path_str.matches('/').count();
                                insert_files.execute_named(named_params! {
                                    ":fts_id": rowid,
                                    ":path": path_str,
                                    ":depth": depth as i64,
                                    ":mode": U64Wrapper(info.permissions),
                                    ":uid": U64Wrapper(info.user_id),
                                    ":gid": U64Wrapper(info.group_id),
                                    ":atime": info.accessed.timestamp_nanos(),
                                    ":mtime": info.modified.timestamp_nanos(),
                                    ":ctime": info.created.timestamp_nanos(),
                                    ":type": info.filetype.to_num(),
                                    ":length": U64Wrapper(info.length),
                                })?;
                                let inserted_id = transaction.last_insert_rowid();

                                reindex.push(AffectedMacros::New {
                                    file_id: inserted_id,
                                    info,
                                });
                            }
                        }
                    }
                }
            }
        }
        transaction.commit()?;

        Ok(reindex)
    }
}
