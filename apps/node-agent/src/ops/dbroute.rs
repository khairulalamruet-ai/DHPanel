//! Which database server an operation is for (T07.12, T07.13): the node's MariaDB (what
//! every operation meant before) or its PostgreSQL. The operation says; this module hands it to
//! the code of that server.

use std::sync::Arc;

use dhpanel_node_protocol::{AgentError, DbEngine, ErrorCode, Operation, Output};

use super::{db, pg, tenant::TenantContext};

/// The engine of an operation, refusing what this node cannot do.
fn engine(op: &Operation) -> Result<DbEngine, AgentError> {
    match op.db_target() {
        Some((_, Some(_))) => Err(AgentError::new(
            ErrorCode::InvalidArguments,
            "a tenant's own database server is not supported by this node",
        )),
        Some((engine, None)) => Ok(engine),
        None => Ok(DbEngine::Mysql),
    }
}

macro_rules! route {
    ($name:ident, $mysql:path, $postgres:path) => {
        /// Hands the operation to the server it is for.
        ///
        /// # Errors
        /// Whatever that server's code answers.
        pub async fn $name(
            context: Arc<TenantContext>,
            op: Operation,
        ) -> Result<Output, AgentError> {
            match engine(&op)? {
                DbEngine::Mysql => $mysql(context, op).await,
                DbEngine::Postgres => $postgres(context, op).await,
            }
        }
    };
}

route!(create_database, db::create_database, pg::create_database);
route!(drop_database, db::drop_database, pg::drop_database);
route!(upsert_db_user, db::upsert_db_user, pg::upsert_db_user);
route!(grant_db, db::grant_db, pg::grant_db);
route!(revoke_db, db::revoke_db, pg::revoke_db);
route!(
    set_db_remote_hosts,
    db::set_db_remote_hosts,
    pg::set_db_remote_hosts
);
route!(
    set_db_remote_access,
    db::set_db_remote_access,
    pg::set_db_remote_access
);
route!(drop_db_user, db::drop_db_user, pg::drop_db_user);
route!(
    allow_tenant_databases,
    db::allow_tenant_databases,
    pg::allow_tenant
);
