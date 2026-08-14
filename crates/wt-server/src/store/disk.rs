use super::{insert_result, StoreError};
use crate::schema::{disk_nodes, guests};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use uuid::Uuid;

#[derive(Insertable)]
#[diesel(table_name = disk_nodes)]
struct NewDiskNode {
    id: String,
    parent_id: Option<String>,
    immutable: bool,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = disk_nodes)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct DiskNodeRow {
    id: String,
    parent_id: Option<String>,
    immutable: bool,
}

pub(super) fn insert_disk(
    connection: &mut SqliteConnection,
    id: Uuid,
    parent_id: Option<Uuid>,
    immutable: bool,
) -> Result<(), StoreError> {
    let row = NewDiskNode {
        id: id.to_string(),
        parent_id: parent_id.map(|id| id.to_string()),
        immutable,
    };
    insert_result(
        diesel::insert_into(disk_nodes::table)
            .values(row)
            .execute(connection),
    )
}

pub(super) fn garbage_for_delete(
    connection: &mut SqliteConnection,
    instance_id: Uuid,
) -> Result<Vec<Uuid>, StoreError> {
    let mut current = guests::table
        .find(instance_id.to_string())
        .select(guests::head_disk_id)
        .first::<String>(connection)
        .optional()?
        .ok_or(StoreError::NotFound)?;
    let mut garbage = Vec::new();
    loop {
        let node = disk_nodes::table
            .find(&current)
            .select(DiskNodeRow::as_select())
            .first::<DiskNodeRow>(connection)?;
        if (garbage.is_empty() && node.immutable) || (!garbage.is_empty() && !node.immutable) {
            return Err(StoreError::InvalidData(
                "disk graph mutability invariant is broken".into(),
            ));
        }
        garbage.push(
            Uuid::parse_str(&node.id)
                .map_err(|error| StoreError::InvalidData(error.to_string()))?,
        );
        let Some(parent_id) = node.parent_id else {
            break;
        };
        let other_children = disk_nodes::table
            .filter(disk_nodes::parent_id.eq(&parent_id))
            .filter(disk_nodes::id.ne(&current))
            .count()
            .get_result::<i64>(connection)?;
        let direct_heads = guests::table
            .filter(guests::head_disk_id.eq(&parent_id))
            .count()
            .get_result::<i64>(connection)?;
        if other_children != 0 || direct_heads != 0 {
            break;
        }
        current = parent_id;
    }
    Ok(garbage)
}
