use anyhow::Result;
use bytemuck;
use rust_decimal::Decimal;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use spl_token_2022::{
    extension::{
        interest_bearing_mint::InterestBearingConfig, BaseStateWithExtensions,
        StateWithExtensionsOwned,
    },
    state::Mint as Mint2022,
};
use stablebond_sdk::{
    accounts::{Bond, PaymentFeed},
    find_bond_pda, find_payment_feed_pda,
};
use std::str::FromStr;
use switchboard_on_demand::on_demand::accounts::pull_feed::PullFeedAccountData;
#[tokio::main]
async fn main() -> Result<()> {
    let mints: Vec<Pubkey> = vec![
        Pubkey::from_str("CETES7CKqqKQizuSN6iWQwmTeFRjbJR6Vw2XRKfEDR8f").unwrap(),
        Pubkey::from_str("USTRYnGgcHAhdWsanv8BG6vHGd4p7UGgoB9NRd8ei7j").unwrap(),
        Pubkey::from_str("EuroszHk1AL7fHBBsxgeGHsamUqwBpb26oEyt9BcfZ6G").unwrap(),
        Pubkey::from_str("BRNTNaZeTJANz9PeuD8drNbBHwGgg7ZTjiQYrFgWQ48p").unwrap(),
        Pubkey::from_str("GiLTSeSFnNse7xQVYeKdMyckGw66AoRmyggGg1NNd4yr").unwrap(),
    ];

    for mint in mints {
        let bond_account = find_bond_pda(mint).0;
        let client = RpcClient::new_with_commitment(
            "https://rpc.etherfuse.com".to_string(),
            CommitmentConfig::processed(),
        );

        // Given the bond, receive the payment feed accounts used to calculate the price
        let data = client.get_account_data(&bond_account).await?;
        let bond = Bond::from_bytes(&data)?;
        println!("\n============= BOND: {:?} =============", bond.mint);
        let payment_feed_account = find_payment_feed_pda(bond.payment_feed_type).0;
        let data = client.get_account_data(&payment_feed_account).await?;
        let payment_feed = PaymentFeed::from_bytes(&data)?;
        let base_price_feed = payment_feed.base_price_feed;
        let quote_price_feed = payment_feed.quote_price_feed;

        let data = client.get_account_data(&base_price_feed).await?;
        let mut aligned_data = vec![0u8; std::mem::size_of::<PullFeedAccountData>()];
        aligned_data.copy_from_slice(&data[8..std::mem::size_of::<PullFeedAccountData>() + 8]);
        let pull_feed = bytemuck::try_from_bytes::<PullFeedAccountData>(&aligned_data)
            .map_err(|e| anyhow::anyhow!("{:?}", e))?;
        let base_price_decimal = pull_feed.value().unwrap();
        println!("Base Price Decimal: {:?}", base_price_decimal);

        // If the quote price feed is not the default, we need to use it to calculate the price
        let price: Decimal;
        if quote_price_feed != Pubkey::default() {
            let data = client.get_account_data(&quote_price_feed).await?;
            let mut aligned_data = vec![0u8; std::mem::size_of::<PullFeedAccountData>()];
            aligned_data.copy_from_slice(&data[8..std::mem::size_of::<PullFeedAccountData>() + 8]);
            let pull_feed = bytemuck::try_from_bytes::<PullFeedAccountData>(&aligned_data)
                .map_err(|e| anyhow::anyhow!("{:?}", e))?;
            let quote_price_decimal = pull_feed.value().unwrap();
            println!("Quote Price Decimal: {:?}", quote_price_decimal);
            price = base_price_decimal * quote_price_decimal;
        } else {
            price = base_price_decimal;
        }
        println!("Price: {:?}", price);

        // Token 22 IB extension data
        let mint_account = client.get_account_data(&mint).await?;
        let state = StateWithExtensionsOwned::<Mint2022>::unpack(mint_account)?;
        // stablebonds are 6 decimals. Hardcoding this for now.
        let unix_timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        // stablebonds are 6 decimals. Hardcoding this for now.
        let scaling_factor = 1000000;
        let ui_bond_value = state
            .get_extension::<InterestBearingConfig>()?
            .amount_to_ui_amount(scaling_factor, 6, unix_timestamp)
            .unwrap();
        let ui_bond_value_decimal = Decimal::from_str(&ui_bond_value).unwrap();
        println!("UI Bond Value: {:?}", ui_bond_value_decimal);

        // Calculate the cost in USDC for 1 Bond
        let usdc_price = ui_bond_value_decimal / price;
        println!("Cost in USDC for 1 Bond: {:?}\n", usdc_price);
    }
    Ok(())
}
