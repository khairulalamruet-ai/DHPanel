//! Secrets that are kept out of the operations ledger and put back when an
//! operation is sent to its node (T05.17b).

use dhpanel_auth_core::secretbox::MasterKey;
use dhpanel_dispatcher::SecretFiller;
use dhpanel_node_protocol::Operation;

/// Fills in the private keys of uploaded certificates: the ledger has the chain
/// only, the sealed key is opened here, for the one frame that carries it. A
/// certificate that was removed since the operation was queued is left out (the
/// removal has already queued the next operation).
pub fn certificate_keys(db: dhpanel_database::sqlx::PgPool, master_key: MasterKey) -> SecretFiller {
    std::sync::Arc::new(move |op| {
        let (db, master_key) = (db.clone(), master_key.clone());
        Box::pin(async move {
            let Operation::SetCustomCertificates {
                username,
                mut items,
            } = op
            else {
                return Ok(op);
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
