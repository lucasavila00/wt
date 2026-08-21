use crate::schema::worlds;
use crate::{to_i64, to_u64, RegistryError, Resource, Resources};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use uuid::Uuid;

#[derive(QueryableByName)]
struct ResourceSum {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    vcpus: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    memory_mib: i64,
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    disk_gib: i64,
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = worlds)]
#[diesel(check_for_backend(diesel::sqlite::Sqlite))]
struct ReservationRow {
    vcpus: i64,
    memory_mib: i64,
    disk_gib: i64,
    compute_reserved: bool,
    disk_reserved_gib: i64,
}

pub fn reserved_resources(connection: &mut SqliteConnection) -> Result<Resources, RegistryError> {
    let sum = diesel::sql_query(
        "SELECT COALESCE(SUM(CASE WHEN compute_reserved THEN vcpus ELSE 0 END), 0) AS vcpus, COALESCE(SUM(CASE WHEN compute_reserved THEN memory_mib ELSE 0 END), 0) AS memory_mib, COALESCE(SUM(disk_reserved_gib), 0) AS disk_gib FROM worlds",
    )
    .get_result::<ResourceSum>(connection)?;
    Ok(Resources {
        vcpus: to_u64(sum.vcpus, "vcpus")?,
        memory_mib: to_u64(sum.memory_mib, "memory_mib")?,
        disk_gib: to_u64(sum.disk_gib, "disk_gib")?,
    })
}

pub(crate) fn ensure_capacity(
    connection: &mut SqliteConnection,
    requested: Resources,
    limit: Resources,
) -> Result<(), RegistryError> {
    if requested.vcpus == 0 || requested.memory_mib == 0 || requested.disk_gib == 0 {
        return Err(RegistryError::ZeroResources);
    }
    let reserved = reserved_resources(connection)?;
    for (resource, reserved, requested, total) in [
        (
            Resource::Memory,
            reserved.memory_mib,
            requested.memory_mib,
            limit.memory_mib,
        ),
        (Resource::Cpu, reserved.vcpus, requested.vcpus, limit.vcpus),
        (
            Resource::Disk,
            reserved.disk_gib,
            requested.disk_gib,
            limit.disk_gib,
        ),
    ] {
        if reserved
            .checked_add(requested)
            .is_none_or(|sum| sum > total)
        {
            return Err(RegistryError::Capacity {
                resource,
                total,
                reserved,
                requested,
            });
        }
    }
    Ok(())
}

pub fn reserve_resources(
    connection: &mut SqliteConnection,
    id: Uuid,
    limit: Resources,
) -> Result<(), RegistryError> {
    let row = worlds::table
        .find(id.to_string())
        .select(ReservationRow::as_select())
        .first::<ReservationRow>(connection)?;
    let disk_gib = to_u64(row.disk_gib, "disk_gib")?;
    let disk_reserved_gib = to_u64(row.disk_reserved_gib, "disk_reserved_gib")?;
    let disk_target_gib = disk_gib.max(disk_reserved_gib);
    if row.compute_reserved && disk_reserved_gib == disk_target_gib {
        return Ok(());
    }
    let reserved = reserved_resources(connection)?;
    let mut requests = vec![(
        Resource::Disk,
        reserved.disk_gib,
        disk_target_gib - disk_reserved_gib,
        limit.disk_gib,
    )];
    if !row.compute_reserved {
        requests.extend([
            (
                Resource::Cpu,
                reserved.vcpus,
                to_u64(row.vcpus, "vcpus")?,
                limit.vcpus,
            ),
            (
                Resource::Memory,
                reserved.memory_mib,
                to_u64(row.memory_mib, "memory_mib")?,
                limit.memory_mib,
            ),
        ]);
    }
    for (resource, reserved, requested, total) in requests {
        if reserved
            .checked_add(requested)
            .is_none_or(|sum| sum > total)
        {
            return Err(RegistryError::Capacity {
                resource,
                total,
                reserved,
                requested,
            });
        }
    }
    diesel::update(worlds::table.find(id.to_string()))
        .set((
            worlds::compute_reserved.eq(true),
            worlds::disk_reserved_gib.eq(to_i64(disk_target_gib, "disk_reserved_gib")?),
        ))
        .execute(connection)?;
    Ok(())
}

pub fn release_resources(
    connection: &mut SqliteConnection,
    id: Uuid,
    disk_usage_bytes: u64,
) -> Result<(), RegistryError> {
    let disk_reserved_gib = disk_usage_bytes.div_ceil(1024 * 1024 * 1024);
    let changed = diesel::update(worlds::table.find(id.to_string()))
        .set((
            worlds::compute_reserved.eq(false),
            worlds::disk_reserved_gib.eq(to_i64(disk_reserved_gib, "disk_reserved_gib")?),
        ))
        .execute(connection)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(RegistryError::InvalidData("guest not found".into()))
    }
}

pub fn ensure_resources_reserved(
    connection: &mut SqliteConnection,
    id: Uuid,
) -> Result<(), RegistryError> {
    let row = worlds::table
        .find(id.to_string())
        .select(ReservationRow::as_select())
        .first::<ReservationRow>(connection)?;
    let disk_gib = to_u64(row.disk_gib, "disk_gib")?;
    let disk_reserved_gib = to_u64(row.disk_reserved_gib, "disk_reserved_gib")?;
    let changed = diesel::update(worlds::table.find(id.to_string()))
        .set((
            worlds::compute_reserved.eq(true),
            worlds::disk_reserved_gib.eq(to_i64(
                disk_gib.max(disk_reserved_gib),
                "disk_reserved_gib",
            )?),
        ))
        .execute(connection)?;
    if changed == 1 {
        Ok(())
    } else {
        Err(RegistryError::InvalidData("guest not found".into()))
    }
}
