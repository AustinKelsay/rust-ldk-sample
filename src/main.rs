use ldk_node::bitcoin::Network;
use ldk_node::Builder;
use ldk_node::Event;
use std::str::FromStr;
use std::thread;
use std::sync::Arc;
use lnurl::Builder as LnurlBuilder;
use lnurl::lightning_address::LightningAddress;
use ldk_node::lightning_invoice;
use lnurl::LnUrlResponse;

fn main() {
    let mut rl = rustyline::DefaultEditor::new().unwrap();
    let mut builder = Builder::new();
    builder.set_network(Network::Signet);
    builder.set_chain_source_esplora("https://mutinynet.com/api".to_string(), None);
    builder.set_gossip_source_rgs("https://rgs.mutinynet.com/snapshot".to_string());

    let node = builder.build().unwrap();

    node.start().unwrap();

    // Initialize the LNURL client
    let client = LnurlBuilder::default().build_blocking().unwrap();

    // start a new thread to run in background
    // Create a thread-safe reference to the node
    let event_handler_node = Arc::new(node);
    let node_clone = Arc::clone(&event_handler_node);
    
    // Spawn the event handling thread
    thread::spawn(move || {
        loop {
            // Check if there's an event available
            if let Some(event) = node_clone.next_event() {
                match event {
                    Event::PaymentSuccessful { payment_id, .. } => {
                        println!("🎉 Payment successful: {:?}", payment_id);
                    }
                    Event::PaymentFailed { payment_id, reason, .. } => {
                        println!("❌ Payment failed: {:?}, reason: {:?}", payment_id, reason);
                    }
                    Event::ChannelPending { .. } => {
                        println!("⏳ Channel pending confirmation");
                    }
                    Event::ChannelReady { .. } => {
                        println!("✅ Channel ready");
                    }
                    Event::ChannelClosed { .. } => {
                        println!("🔒 Channel closed");
                    }
                    _ => {
                        println!("📣 Event received: {:?}", event);
                    }
                }
                // Mark the event as handled
                node_clone.event_handled();
            }
            
            // Small sleep to prevent CPU spinning
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    });

    loop {
        let readline = rl.readline(">> ");
        match readline {
            Ok(line) => {
                let words = line.trim().split_whitespace().collect::<Vec<_>>();
                if words.is_empty() {
                    continue;
                }
                match words[0] {
                    "exit" => {
                        return;
                    }
                    "address" => {
                        let addr = event_handler_node.onchain_payment().new_address().unwrap();
                        println!("{}", addr);
                    }
                    "balance" => {
                        let balance = event_handler_node.list_balances();
                        println!("{:?}", balance);
                    }
                    "sync" => {
                        let sync = event_handler_node.sync_wallets();
                        println!("{:?}", sync);
                    }
                    "open" => {
                        if words.len() > 3 {
                            // parse out fields
                            // node_id word 1 secp256k1 public key
                            let node_id = match words[1].parse() {
                                Ok(id) => id,
                                Err(_) => {
                                    println!("Invalid node ID format");
                                    continue;
                                }
                            };
                            
                            // address word 2
                            let address = match words[2].parse() {
                                Ok(addr) => addr,
                                Err(_) => {
                                    println!("Invalid address format");
                                    continue;
                                }
                            };
                            
                            // amount word 3
                            let amount_sats = match u64::from_str(words[3]) {
                                Ok(amt) => amt,
                                Err(_) => {
                                    println!("Invalid amount format");
                                    continue;
                                }
                            };
                            
                            match event_handler_node.open_channel(node_id, address, amount_sats, None, None) {
                                Ok(channel_id) => println!("Channel opened successfully: {:?}", channel_id),
                                Err(e) => println!("Failed to open channel: {:?}", e),
                            }
                        } else {
                            println!("Usage: open <node_id> <address> <amount_sats>");
                        }
                    }
                    "send" => {
                        if words.len() > 2 {
                            let lightning_address_str = words[1];
                            let amount_msats = match u64::from_str(words[2]) {
                                Ok(amt) => amt * 1000, // Convert sats to msats
                                Err(_) => {
                                    println!("Invalid amount format");
                                    continue;
                                }
                            };

                            let ln_addr = match LightningAddress::from_str(lightning_address_str) {
                                Ok(addr) => addr,
                                Err(e) => {
                                    println!("Invalid Lightning Address format: {:?}", e);
                                    continue;
                                }
                            };

                            let url = match ln_addr.lnurlp_url() {
                                Ok(url) => url,
                                Err(_) => {
                                    println!("Failed to get LNURL from address");
                                    continue;
                                }
                            };

                            match client.make_request(&url.to_string()) {
                                Ok(LnUrlResponse::LnUrlPayResponse(pay_response)) => {
                                    match client.get_invoice(&pay_response, amount_msats, None, None) {
                                        Ok(invoice_data) => {
                                            match lightning_invoice::Bolt11Invoice::from_str(invoice_data.invoice()) {
                                                Ok(invoice) => {
                                                    match event_handler_node.bolt11_payment().send(&invoice, None) {
                                                        Ok(payment_id) => println!("💸 Payment initiated with ID: {:?}", payment_id),
                                                        Err(e) => println!("Failed to send payment: {:?}", e),
                                                    }
                                                }
                                                Err(e) => println!("Failed to parse invoice: {:?}", e),
                                            }
                                        }
                                        Err(e) => println!("Failed to get invoice from LNURL service: {:?}", e),
                                    }
                                }
                                Ok(_) => println!("Unexpected LNURL response type"),
                                Err(e) => println!("LNURL request failed: {:?}", e),
                            }
                        } else {
                            println!("Usage: send <lightning_address> <amount_sats>");
                        }
                    }
                    &_ => {
                        println!("Unknown command: {}", words[0]);
                    }
                }
            }
            Err(_) => return,
        }
    }
}
