use std::collections::HashMap;
use std::io::Write;

use starknet::macros::selector;
use starknet::{
    core::types::{BlockId, EmittedEvent, EventFilter, Felt},
    macros::felt,
    providers::{jsonrpc::HttpTransport, AnyProvider, JsonRpcClient, Provider},
};
use url::Url;

const NUMS_TOTALS_SELECTOR: Felt =
    felt!("0x293104e49f49ee445423ae4b6ed9cbcc84ce3e5a27466264fee006dea23bfa6");

#[tokio::main]
async fn main() {
    let mainnet_rpc_url = Url::parse("https://api.cartridge.gg/x/starknet/mainnet")
        .expect("Expecting Starknet RPC URL");

    let appchain_rpc_url = Url::parse("http://localhost:5050").expect("Expecting Starknet RPC URL");

    let piltover_mainnet =
        Felt::from_hex("0x0005edcd6d607a9f83184fda3462cb7b0bd6dbf41942ecb1fca10d76ebbc06cf")
            .unwrap();

    let world_appchain_contract =
        Felt::from_hex("0x7686a16189676ac3978c3b865ae7e3d625a1cd7438800849c7fd866e4b9afd1")
            .unwrap();

    let provider_mainnet = AnyProvider::JsonRpcHttp(JsonRpcClient::new(HttpTransport::new(
        mainnet_rpc_url.clone(),
    )));

    let provider_appchain = AnyProvider::JsonRpcHttp(JsonRpcClient::new(HttpTransport::new(
        appchain_rpc_url.clone(),
    )));

    let mainnet_filter = EventFilter {
        from_block: Some(BlockId::Number(1180290_u64.into())),
        to_block: None,
        address: Some(piltover_mainnet),
        keys: None,
    };

    let appchain_filter = EventFilter {
        from_block: None,
        to_block: None,
        address: Some(world_appchain_contract),
        keys: None,
    };

    println!("Gathering appchain events");
    let appchain_events = get_all_events(provider_appchain, appchain_filter).await;

    println!("Gathering mainnet events");
    let mainnet_events = get_all_events(provider_mainnet, mainnet_filter).await;

    let mut appchain_player_rewards = HashMap::new();
    let mut mainnet_player_claims = HashMap::new();

    for e in appchain_events {
        if e.keys[0] == selector!("StoreSetRecord") && e.keys[1] == NUMS_TOTALS_SELECTOR {
            // https://github.com/cartridge-gg/nums/blob/d7ab1568b34308ea8f40bb70e2ce8e64a62922b8/contracts/appchain/src/systems/claim_actions.cairo#L85
            // https://github.com/cartridge-gg/nums/blob/d7ab1568b34308ea8f40bb70e2ce8e64a62922b8/contracts/appchain/src/models/totals.cairo#L5

            // Player address is the only key.
            // data[0] = length of keys (0x1).
            let player_address = e.data[1];

            // The rewards earned is the first data.
            // data[2] = length of data (0x4).
            let rewards_earned = e.data[3];

            appchain_player_rewards
                .entry(player_address)
                .and_modify(|e| *e += rewards_earned)
                .or_insert(rewards_earned);
        }
    }

    for e in mainnet_events {
        if e.keys[0] == selector!("MessageConsumed") {
            // https://github.com/cartridge-gg/nums/blob/d7ab1568b34308ea8f40bb70e2ce8e64a62922b8/contracts/starknet/dojo/src/systems/message_consumers.cairo#L72
            // Index 0 is length of data (0x3).
            // Index 1 is the player address.
            // Index 2 is the game id.
            // Index 3 is the amount of rewards earned.
            let player_address = e.data[1];
            let rewards_earned = e.data[3];

            mainnet_player_claims
                .entry(player_address)
                .and_modify(|e| *e += rewards_earned)
                .or_insert(rewards_earned);
        }
    }

    println!(
        "Appchain player rewards: {:?}",
        appchain_player_rewards.len()
    );

    println!("Mainnet player claims: {:?}", mainnet_player_claims.len());

    let mut airdrop = std::fs::File::create("/tmp/nums_airdrop_sequence1.csv").unwrap();
    let mut listing = std::fs::File::create("/tmp/nums_rewards_claims.csv").unwrap();

    writeln!(
        listing,
        "\"Player address\",\"Earned on appchain\",\"Claimed on mainnet\""
    )
    .unwrap();
    writeln!(airdrop, "\"Player address\",\"Amount to airdrop\"").unwrap();

    let mut n_airdrop = 0;

    // Can we iterate on ordered player addresses?
    let mut player_addresses = appchain_player_rewards.keys().collect::<Vec<_>>();
    player_addresses.sort();

    for player_address in player_addresses {
        let rewards_earned = appchain_player_rewards.get(player_address).unwrap();
        let claims_balance = mainnet_player_claims
            .get(&player_address)
            .unwrap_or(&Felt::ZERO);

        /* println!(
            "Player address: {:?}, Earned on appchain: {}, Claimed on mainnet: {}",
            player_address,
            rewards_earned,
            claims_balance
        ); */

        writeln!(
            listing,
            "{:#066x},{},{}",
            player_address, rewards_earned, claims_balance
        )
        .unwrap();

        if rewards_earned > claims_balance {
            let amount_to_airdrop = rewards_earned - claims_balance;
            writeln!(airdrop, "{:#066x},{}", player_address, amount_to_airdrop).unwrap();
            n_airdrop += 1;
        }
    }

    println!("Number of airdropped players: {}", n_airdrop);
}

/// Get all events from a provider for a given filter.
///
/// This function will iterate on all event pages to retrieve all events.
async fn get_all_events<P: Provider>(provider: P, filter: EventFilter) -> Vec<EmittedEvent> {
    let mut events = vec![];

    let mut ct = None;

    loop {
        let ep = provider.get_events(filter.clone(), ct, 500).await.unwrap();
        events.extend(ep.events.clone());

        if ep.events.len() == 0 {
            break;
        }

        if let Some(ep_ct) = ep.continuation_token {
            ct = Some(ep_ct);
        } else {
            break;
        }
    }

    events
}
