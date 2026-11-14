//! Secrets that are kept out of the operations ledger and put back when an
//! operation is sent to its node (T05.17b).

use dhpanel_auth_core::secretbox::MasterKey;
use dhpanel_dispatcher::SecretFiller;
use dhpanel_node_protocol::Operation;

/// Fills in the private keys of uploaded certificates: the ledger has the chain
/// only, the sealed key is opened here, for the one frame that carries it. A
/// certificate that was removed since the operation was queued is left out (the
/// removal has already queued the next operation).
pub fn operation_secrets(
    db: dhpanel_database::sqlx::PgPool,
    master_key: MasterKey,
) -> SecretFiller {
    std::sync::Arc::new(move |op| {
        let (db, master_key) = (db.clone(), master_key.clone());
        Box::pin(async move {
            let (username, mut items) = match op {
                Operation::SetCustomCertificates { username, items } => (username, items),
                // A database user's password: sealed by the API, opened here for this
                // one message. Without one (it was delivered already) the node keeps
                // the password the user has.
                Operation::UpsertDbUser {
                    username,
                    name,
                    password: None,
                } => {
                    let found = dhpanel_database::databases::sealed_password_of(&db, &name)
                        .await
                        .map_err(|_| "could not read a database password".to_owned())?;
                    let password = match found {
                        Some((user_id, sealed)) => {
                            let bytes = master_key
                                .open(
                                    &dhpanel_database::databases::password_context(user_id),
                                    &sealed,
                                )
                                .map_err(|_| "a database password does not open".to_owned())?;
                            Some(dhpanel_node_protocol::DbPassword(
                                String::from_utf8(bytes)
                                    .map_err(|_| "a database password is not text".to_owned())?,
                            ))
                        }
                        None => None,
                    };
                    return Ok(Operation::UpsertDbUser {
                        username,
                        name,
                        password,
                    });
                }
                other => return Ok(other),
            };
            let mut filled = Vec::with_capacity(items.len());
            for mut item in items.drain(..) {
                if item.private_key.is_none() {
                    let found =
                        dhpanel_database::certificates::custom_key_of(&db, &username, &item.host)
                            .await
                            .map_err(|_| "could not read a certificate key".to_owned())?;
                    let Some((domain_id, sealed)) = found else {
                        continue;
                    };
                    let key = master_key
                        .open(
                            &dhpanel_database::certificates::key_context(domain_id),
                            &sealed,
                        )
                        .map_err(|_| "a certificate key does not open".to_owned())?;
                    item.private_key = Some(
                        String::from_utf8(key)
                            .map_err(|_| "a certificate key is not text".to_owned())?,
                    );
                }
                filled.push(item);
            }
            Ok(Operation::SetCustomCertificates {
                username,
                items: filled,
            })
        })
    })
}
