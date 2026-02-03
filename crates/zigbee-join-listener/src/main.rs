#![allow(clippy::doc_markdown)]
//! Zigbee Join Listener / Diagnostic Tool
//!
//! A simple CLI tool that checks the ConBee II firmware and network state,
//! enables permit join mode, and listens for new devices joining.

use deconz_protocol::{
    ApsDataIndication, CommandId, DeconzEvent, DeconzTransport, NetworkParameter,
};
use std::env;
use tokio::sync::broadcast;
use tracing::info;

fn init_tracing() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("zigbee_join_listener=info".parse()?)
                .add_directive("deconz_protocol=debug".parse()?),
        )
        .init();
    Ok(())
}

async fn print_firmware_info(transport: &DeconzTransport) {
    println!("--- Firmware & Device Info ---");

    match transport
        .request(CommandId::Version, vec![0x00, 0x00, 0x00])
        .await
    {
        Ok(frame) => {
            println!("Version response: {:02X?}", frame.payload);
            if frame.payload.len() >= 4 {
                let version = u32::from_le_bytes([
                    frame.payload[0],
                    frame.payload[1],
                    frame.payload.get(2).copied().unwrap_or(0),
                    frame.payload.get(3).copied().unwrap_or(0),
                ]);
                println!("  Raw version: {version:#010X}");
                println!("  Major: {}", (version >> 24) & 0xFF);
                println!("  Minor: {}", (version >> 16) & 0xFF);
                println!("  Patch: {}", (version >> 8) & 0xFF);
                println!("  Platform: {}", version & 0xFF);
            }
        }
        Err(e) => {
            println!("  Version command failed: {e}");
        }
    }

    match transport.read_parameter(NetworkParameter::MacAddress).await {
        Ok(mac) => {
            let mac_str = mac
                .iter()
                .rev()
                .map(|b| format!("{b:02x}"))
                .collect::<Vec<_>>()
                .join(":");
            println!("  MAC Address: {mac_str}");
        }
        Err(e) => println!("  MAC Address: error - {e}"),
    }
}

async fn print_network_state(transport: &DeconzTransport) {
    println!();
    println!("--- Network State ---");

    match transport.get_device_state().await {
        Ok(state) => {
            println!("  Network State: {:?}", state.network_state);
            println!("  APS Data Confirm: {}", state.aps_data_confirm);
            println!("  APS Data Indication: {}", state.aps_data_indication);
            println!("  Config Changed: {}", state.configuration_changed);
            println!("  APS Request Free Slots: {}", state.aps_request_free_slots);
        }
        Err(e) => println!("  Device state error: {e}"),
    }

    match transport
        .read_parameter(NetworkParameter::CurrentChannel)
        .await
    {
        Ok(ch) => println!("  Channel: {}", ch.first().copied().unwrap_or(0)),
        Err(e) => println!("  Channel: error - {e}"),
    }

    match transport.read_parameter(NetworkParameter::NwkPanId).await {
        Ok(pan) => {
            if pan.len() >= 2 {
                let pan_id = u16::from_le_bytes([pan[0], pan[1]]);
                println!("  PAN ID: {pan_id:#06x}");
            }
        }
        Err(e) => println!("  PAN ID: error - {e}"),
    }

    match transport.read_parameter(NetworkParameter::NwkAddress).await {
        Ok(addr) => {
            if addr.len() >= 2 {
                let nwk_addr = u16::from_le_bytes([addr[0], addr[1]]);
                println!("  Network Address: {nwk_addr:#06x}");
            }
        }
        Err(e) => println!("  Network Address: error - {e}"),
    }

    match transport.read_parameter(NetworkParameter::PermitJoin).await {
        Ok(pj) => {
            let duration = pj.first().copied().unwrap_or(0);
            println!("  Permit Join: {} ({})", duration > 0, duration);
        }
        Err(e) => println!("  Permit Join: error - {e}"),
    }

    match transport
        .read_parameter(NetworkParameter::SecurityMode)
        .await
    {
        Ok(sm) => println!("  Security Mode: {:#04x}", sm.first().copied().unwrap_or(0)),
        Err(e) => println!("  Security Mode: error - {e}"),
    }
}

async fn enable_permit_join(transport: &DeconzTransport) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("--- Enabling Permit Join ---");

    let join_duration: u8 = env::var("JOIN_DURATION")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(254);

    transport
        .write_parameter(NetworkParameter::PermitJoin, &[join_duration])
        .await?;
    println!("  Permit join enabled for {join_duration} seconds");
    Ok(())
}

fn handle_device_announced(ieee_addr: [u8; 8], short_addr: u16, capability: u8) {
    let ieee_str = ApsDataIndication::format_ieee(&ieee_addr);
    let is_router = (capability & 0x02) != 0;
    let is_mains = (capability & 0x04) != 0;

    println!("****************************************");
    println!("*** NEW DEVICE JOINED! ***");
    println!("****************************************");
    println!("  IEEE Address:  {ieee_str}");
    println!("  Short Address: {short_addr:#06x}");
    println!("  Capability:    {capability:#04x}");
    println!(
        "  Type:          {}",
        if is_router { "Router" } else { "End Device" }
    );
    println!(
        "  Power:         {}",
        if is_mains { "Mains" } else { "Battery" }
    );
    println!("****************************************");
    println!();
}

fn handle_aps_indication(indication: &ApsDataIndication) {
    println!(
        "APS Indication: profile={:#06x} cluster={:#06x} src={:#06x} ep={}",
        indication.profile_id,
        indication.cluster_id,
        indication.src_short_addr,
        indication.src_endpoint
    );

    if indication.profile_id == 0x0000 && indication.cluster_id == 0x0013 {
        println!("  -> Device Announce ZDO message!");
    }
}

async fn run_event_loop(
    transport: &DeconzTransport,
    mut rx: broadcast::Receiver<DeconzEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        match rx.recv().await {
            Ok(DeconzEvent::DeviceAnnounced {
                ieee_addr,
                short_addr,
                capability,
            }) => handle_device_announced(ieee_addr, short_addr, capability),
            Ok(DeconzEvent::DeviceStateChanged(state)) => {
                if state.aps_data_indication {
                    info!("APS data available, requesting...");
                    if let Err(e) = transport.request_aps_data().await {
                        info!("Failed to request APS data: {}", e);
                    }
                }
            }
            Ok(DeconzEvent::ApsIndication(indication)) => handle_aps_indication(&indication),
            Ok(DeconzEvent::ApsDataAvailable) => {
                info!("APS data available event");
                if let Err(e) = transport.request_aps_data().await {
                    info!("Failed to request APS data: {}", e);
                }
            }
            Ok(_) => {}
            Err(broadcast::error::RecvError::Lagged(n)) => {
                info!("Missed {} events", n);
            }
            Err(broadcast::error::RecvError::Closed) => {
                println!("Event channel closed");
                break;
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_tracing()?;

    let serial_port = env::var("CONBEE_PORT").unwrap_or_else(|_| "/dev/ttyUSB0".to_string());

    println!("========================================");
    println!("   ConBee II Diagnostic & Join Listener");
    println!("========================================");
    println!();

    info!("Connecting to ConBee II at {}", serial_port);
    let transport = DeconzTransport::connect(&serial_port)?;

    print_firmware_info(&transport).await;
    print_network_state(&transport).await;
    enable_permit_join(&transport).await?;

    println!();
    println!("========================================");
    println!("  Listening for new devices...");
    println!("  (Press Ctrl+C to exit)");
    println!("========================================");
    println!();

    let rx = transport.subscribe();
    run_event_loop(&transport, rx).await
}
