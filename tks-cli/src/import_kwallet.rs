//! Import KWallet data into Secret Service collections
//! NOTE: should this tool become a KWallet to Secret Service conversion tool on its own?
//!
//! This uses an XML file previously created by the KWalletManager's `export to XML` function.

use anyhow::{anyhow, Context, Result};
use clap::Parser;
use log::{debug, info, warn};
use roxmltree::NodeType;
use roxmltree::NodeType::Element;
use secret_service::{Collection, EncryptionType, SecretService};
use std::collections::HashMap;
use std::error::Error;
use std::fs;

#[derive(Parser, Debug)]
#[clap(verbatim_doc_comment)]
pub struct ImportKwalletCmd {
    #[clap(verbatim_doc_comment)]
    /// Path to the KWalletManager's exported file
    pub xml_file: String,

    #[clap(long, short = 'd', default_value = "true", verbatim_doc_comment)]
    /// Imports all the wallet's contents into the `default` collection
    pub to_default_collection: bool,

    #[clap(long, verbatim_doc_comment)]
    /// This option excludes the `to_default_collection` option
    pub collection_name: Option<String>,

    #[clap(long, short = 'r', default_value = "false", verbatim_doc_comment)]
    /// This is useful when re-attempting a in the middle stopped import and we need to avoid
    /// duplicate errors
    pub replace_existing_items: bool,
}

impl ImportKwalletCmd {
    pub(crate) async fn run(&self) -> Result<()> {
        info!("Importing kwallet data from file: {}", self.xml_file);
        if self.to_default_collection {
            info!("  target the default collection");
        } else {
            if let Some(collection) = self.collection_name.as_ref() {
                info!("  target the collection: {}", collection);
            }
        }
        let xml_string = fs::read_to_string(&self.xml_file)
            .with_context(|| format!("Error reading file '{}'", self.xml_file))?;

        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .unwrap_or_else(|err| {
                panic!("  Failed to connect to secret service. Is the TKS service running?");
            });
        let collection = if self.to_default_collection {
            ss.get_default_collection()
                .await
                .with_context(|| "Failed to get default collection")?
        } else {
            let cols = ss
                .get_all_collections()
                .await
                .with_context(|| "Failed to get all collections")?;
            let collection_name = &self.collection_name.as_ref().unwrap().clone();
            let mut coll: Option<Collection> = None;
            for c in cols {
                match *collection_name
                    == c.get_label()
                        .await
                        .with_context(|| "Failed to read collection label")?
                {
                    true => coll = Some(c),
                    false => continue,
                }
            }
            coll.ok_or_else(|| anyhow!("No collection named '{}' found", collection_name))?
        };

        if collection
            .is_locked()
            .await
            .with_context(|| "Failed to read collection locked state")?
        {
            collection
                .unlock()
                .await
                .with_context(|| "Failed to unlock collection")?;
        }

        let xml = roxmltree::Document::parse(&xml_string).expect("Import failed");
        if let Some(wallet) = xml.descendants().find(|n| n.tag_name().name() == "wallet") {
            for f in wallet.children().filter(|n| n.node_type() == Element) {
                let current_folder = f
                    .attribute("name")
                    .ok_or_else(|| anyhow!("Missing name in wallet attribute"))?;
                info!("  processing folder '{}'", current_folder);
                for e in f.children().filter(|n| n.node_type() == Element) {
                    debug!("  entry: {:?}", e);

                    let label = e.attribute("name").ok_or_else(|| anyhow!("Missing name"))?;
                    let item_type = e.tag_name().name();
                    match item_type {
                        "map" => {
                            // NOTE: at the time of writing this importer, it is not clear for me
                            // how maps should be represented into the Secret Service in such a way
                            // the client applications seamlessly find the same settings in SS
                            // instead of KWallet
                            info!("    Ignoring map entry {}/{}", current_folder, label);
                        }
                        "password" => {
                            let mut properties = HashMap::new();
                            properties.insert("tks:kwallet-folder", current_folder);
                            properties.insert("tks:kwallet-entry-type", item_type);
                            properties.insert("xdg:schema", "org.freedesktop.Secret.Generic");
                            properties.insert("xdg:creator", "org.kde.KWallet");
                            if let Some(secret_text) = e.text() {
                                let secret: &[u8] = secret_text.as_bytes();
                                // existing items will be updated in the secret service
                                let p = collection
                                    .create_item(
                                        label,
                                        properties,
                                        secret,
                                        self.replace_existing_items,
                                        "text/plain",
                                    )
                                    .await
                                    .with_context(|| {
                                        format!("Failed to create item '{}'", label)
                                    })?;
                                match p.item_path.to_string() == "/" {
                                    true => {
                                        warn!("The Secret Service (maybe TKS) returned a prompt instead of creating item {}", label);
                                    }
                                    false => {
                                        info!(
                                            "  '{}/{}' -> '{}'",
                                            current_folder,
                                            label,
                                            p.item_path.to_string()
                                        );
                                    }
                                }
                            } else {
                                info!(
                                    "  '{}/{}' -> 'None' (as it was empty)",
                                    current_folder, label
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }
        } else {
            panic!("XML file does not contain a wallet root element");
        }
        Ok(())
    }
}
