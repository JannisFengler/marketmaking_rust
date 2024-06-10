#![warn(clippy::all, clippy::nursery, clippy::pedantic)]

use ethers::{
    signers::{LocalWallet, Signer},
    types::H160,
};
use gxhash::{HashMap, HashMapExt};
use log::{error, info};
use tokio::sync::mpsc::unbounded_channel;

use crate::{
    bps_diff, truncate_float, BaseUrl, ClientCancelRequest, ClientLimit, ClientOrder,
    ClientOrderRequest, ExchangeClient, ExchangeDataStatus, ExchangeResponseStatus, InfoClient,
    Message, Subscription, EPSILON,
};

#[derive(Debug)]
pub struct RestingOrder {
    pub oid: u64,
    pub position: f64,
    pub price: f64,
}

pub struct Input {
    pub asset: String,
    pub target_liquidity: f64,
    pub half_spread: u16,
    pub max_bps_diff: u16,
    pub max_absolute_position_size: f64,
    pub decimals: u32,
    pub wallet: LocalWallet,
}

pub struct MarketMaker {
    pub asset: String,
    pub target_liquidity: f64,
    pub half_spread: u16,
    pub max_bps_diff: u16,
    pub max_absolute_position_size: f64,
    pub decimals: u32,
    pub lower_resting: RestingOrder,
    pub upper_resting: RestingOrder,
    pub cur_position: f64,
    pub latest_mid_price: f64,
    pub info_client: InfoClient,
    pub exchange_client: ExchangeClient,
    pub user_address: H160,
    pub active_orders: HashMap<u64, bool>, // Track active order IDs and their buy/sell status
}

impl MarketMaker {
    /// # Errors
    ///
    /// Returns `Err` if the exchange or info clients can't be created.
    pub async fn new(input: Input) -> Result<Self, Box<dyn std::error::Error>> {
        let user_address = input.wallet.address();

        let info_client = InfoClient::new(None, Some(BaseUrl::Mainnet)).await?;
        let exchange_client =
            ExchangeClient::new(None, input.wallet, Some(BaseUrl::Mainnet), None, None).await?;

        let mut market_maker = Self {
            asset: input.asset,
            target_liquidity: input.target_liquidity,
            half_spread: input.half_spread,
            max_bps_diff: input.max_bps_diff,
            max_absolute_position_size: input.max_absolute_position_size,
            decimals: input.decimals,
            lower_resting: RestingOrder {
                oid: 0,
                position: 0.0,
                price: -1.0,
            },
            upper_resting: RestingOrder {
                oid: 0,
                position: 0.0,
                price: -1.0,
            },
            cur_position: 0.0,
            latest_mid_price: -1.0,
            info_client,
            exchange_client,
            user_address,
            active_orders: HashMap::new(),
        };

        // Fetch and update the state with open orders and positions
        market_maker.update_state().await?;

        Ok(market_maker)
    }

    /// Updates state with open orders and positions.
    ///
    /// # Errors
    ///
    /// Returns `Err` if there's an error fetching open orders or the
    /// current position.
    async fn update_state(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.fetch_open_orders().await?;
        self.fetch_current_position().await?;
        Ok(())
    }

    /// Fetches and updates active and resting orders.
    ///
    /// # Errors
    ///
    /// Returns `Err` if there's an error fetching the open orders from the
    /// exchange.
    async fn fetch_open_orders(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let open_orders = self.info_client.open_orders(self.user_address).await?;
        for order in open_orders.into_iter().filter(|o| o.coin == self.asset) {
            self.active_orders.insert(order.oid, order.side == "B");

            let resting_order = RestingOrder {
                oid: order.oid,
                position: order.sz.parse().unwrap_or_default(),
                price: order.limit_px.parse().unwrap_or_default(),
            };

            match order.side.as_str() {
                "B" => self.lower_resting = resting_order,
                _ => self.upper_resting = resting_order,
            }
        }
        Ok(())
    }

    /// Fetches and updates current user position.
    ///
    /// # Errors
    ///
    /// Returns `Err` if there's an error fetching the user state from the
    /// exchange or parsing the position value.
    async fn fetch_current_position(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let user_state = self.info_client.user_state(self.user_address).await?;
        if let Some(position) = user_state
            .asset_positions
            .iter()
            .find(|&pos| pos.type_string == self.asset)
        {
            self.cur_position = position.position.szi.parse()?;
        }
        Ok(())
    }

    pub async fn start(&mut self) {
        let (sender, mut receiver) = unbounded_channel();

        // Subscribe to UserEvents for fills
        if let Err(e) = self
            .info_client
            .subscribe(
                Subscription::UserEvents {
                    user: self.user_address,
                },
                sender.clone(),
            )
            .await
        {
            error!("Error subscribing to UserEvents: {:?}", e);
            return;
        }

        // Subscribe to AllMids so we can market make around the mid price
        if let Err(e) = self
            .info_client
            .subscribe(Subscription::AllMids, sender)
            .await
        {
            error!("Error subscribing to AllMids: {:?}", e);
            return;
        }

        while let Some(message) = receiver.recv().await {
            self.process_message(message).await;
        }
        error!("Receiver stream ended");
    }

    async fn process_message(&mut self, message: Message) {
        match message {
            Message::AllMids(all_mids) => {
                let all_mids = all_mids.data.mids;
                if let Some(mid) = all_mids.get(&self.asset) {
                    if let Ok(mid) = mid.parse::<f64>() {
                        self.latest_mid_price = mid;
                        // Check to see if we need to cancel or place any new orders
                        self.potentially_update().await;
                    } else {
                        error!(
                            "Invalid mid price format for asset {}: {:?}",
                            self.asset, mid
                        );
                    }
                } else {
                    error!("Could not get mid for asset {}: {:?}", self.asset, all_mids);
                }
            }
            Message::User(user_events) => {
                // We haven't seen the first mid price event yet, so just continue
                if self.latest_mid_price < 0.0 {
                    return;
                }
                let fills = user_events.data.fills;
                for fill in fills {
                    if fill.coin == self.asset {
                        let amount: f64 = fill.sz.parse().unwrap();
                        // Update our resting positions whenever we see a fill
                        if fill.side.eq("B") {
                            self.cur_position += amount;
                            if let Some(is_buy) = self.active_orders.remove(&fill.oid) {
                                if is_buy {
                                    self.lower_resting.position -= amount;
                                }
                            }
                            info!("Fill: bought {amount} {}", self.asset);
                        } else {
                            self.cur_position -= amount;
                            if let Some(is_buy) = self.active_orders.remove(&fill.oid) {
                                if !is_buy {
                                    self.upper_resting.position -= amount;
                                }
                            }
                            info!("Fill: sold {amount} {}", self.asset);
                        }
                    }
                }
                // Check to see if we need to cancel or place any new orders
                self.potentially_update().await;
            }
            _ => {
                error!("Unsupported message type: {:?}", message);
            }
        }
    }

    async fn attempt_cancel(&mut self, asset: String, oid: u64) -> bool {
        // Check if the order is still considered active
        if !self.active_orders.contains_key(&oid) {
            info!("Order was never placed, already canceled, or filled: oid={oid}");
            return true; // No need to cancel
        }

        // Attempt to cancel the order
        let cancel = self
            .exchange_client
            .cancel(ClientCancelRequest { asset, oid }, None)
            .await;

        match cancel {
            Ok(cancel) => match cancel {
                ExchangeResponseStatus::Ok(cancel) => {
                    if let Some(cancel) = cancel.data {
                        if cancel.statuses.is_empty() {
                            error!(
                                "Exchange data statuses is empty when canceling: {:?}",
                                cancel
                            );
                        } else {
                            match cancel.statuses[0].clone() {
                                ExchangeDataStatus::Success => {
                                    self.active_orders.remove(&oid); // Remove from active orders
                                    return true;
                                }
                                ExchangeDataStatus::Error(e) => {
                                    error!("Error with canceling: {e}");
                                    if e.contains("Order does not exist")
                                        || e.contains("already canceled")
                                        || e.contains("Order already filled")
                                    {
                                        self.active_orders.remove(&oid); // Remove from active orders
                                        return true;
                                    }
                                }
                                _ => unreachable!(),
                            }
                        }
                    } else {
                        error!(
                            "Exchange response data is empty when canceling: {:?}",
                            cancel
                        );
                    }
                }
                ExchangeResponseStatus::Err(e) => {
                    error!("Error with canceling: {e}");
                    if e.contains("Order does not exist")
                        || e.contains("already canceled")
                        || e.contains("Order already filled")
                    {
                        self.active_orders.remove(&oid); // Remove from active orders
                        return true;
                    }
                }
            },
            Err(e) => error!("Error with canceling: {e}"),
        }
        false
    }

    async fn place_order(
        &mut self,
        asset: String,
        amount: f64,
        price: f64,
        is_buy: bool,
    ) -> (f64, u64) {
        let order = self
            .exchange_client
            .order(
                ClientOrderRequest {
                    asset,
                    is_buy,
                    reduce_only: false,
                    limit_px: price,
                    sz: amount,
                    cloid: None,
                    // Use ALO TIF for post-only
                    order_type: ClientOrder::Limit(ClientLimit {
                        tif: "Alo".to_string(),
                    }),
                },
                None,
            )
            .await;

        match order {
            Ok(order) => match order {
                ExchangeResponseStatus::Ok(order) => {
                    if let Some(order) = order.data {
                        if order.statuses.is_empty() {
                            error!(
                                "Exchange data statuses is empty when placing order: {:?}",
                                order
                            );
                            return (0.0, 0);
                        }
                        match order.statuses[0].clone() {
                            ExchangeDataStatus::Resting(order) => {
                                self.active_orders.insert(order.oid, is_buy);
                                return (amount, order.oid);
                            }
                            ExchangeDataStatus::Error(e) => {
                                if e.contains("Invalid Time in Force") {
                                    // Adjust to Hyperliquid's specific error message
                                    info!("Post-only order rejected. Will retry on next price update.");
                                } else {
                                    error!("Error with placing order: {}", e);
                                }
                            }
                            _ => {}
                        }
                    } else {
                        error!(
                            "Exchange response data is empty when placing order: {:?}",
                            order
                        );
                        return (0.0, 0);
                    }
                }
                ExchangeResponseStatus::Err(e) => {
                    error!("Error with placing order: {}", e);
                }
            },
            Err(e) => error!("Error with placing order: {}", e),
        }

        (0.0, 0) // Order placement failed
    }

    async fn potentially_update(&mut self) {
        let half_spread = (self.latest_mid_price * f64::from(self.half_spread)) / 10000.0;
        // Determine prices to target from the half spread
        let (lower_price, upper_price) = (
            self.latest_mid_price - half_spread,
            self.latest_mid_price + half_spread,
        );
        let (mut lower_price, mut upper_price) = (
            truncate_float(lower_price, self.decimals, true),
            truncate_float(upper_price, self.decimals, false),
        );

        // Rounding optimistically to make our market tighter might cause a weird edge case, so account for that
        if (lower_price - upper_price).abs() < EPSILON {
            lower_price = truncate_float(lower_price, self.decimals, false);
            upper_price = truncate_float(upper_price, self.decimals, true);
        }

        // Determine amounts we can put on the book without exceeding the max absolute position size
        // Consider the current position when calculating order amounts
        let lower_order_amount =
            (self.max_absolute_position_size - self.cur_position).clamp(0.0, self.target_liquidity);
        let upper_order_amount =
            (self.max_absolute_position_size + self.cur_position).clamp(0.0, self.target_liquidity);

        // Determine if we need to cancel the resting order and put a new order up due to deviation
        let lower_change = (lower_order_amount - self.lower_resting.position).abs() > EPSILON
            || bps_diff(lower_price, self.lower_resting.price) > self.max_bps_diff;
        let upper_change = (upper_order_amount - self.upper_resting.position).abs() > EPSILON
            || bps_diff(upper_price, self.upper_resting.price) > self.max_bps_diff;

        // Consider cancelling
        if self.lower_resting.oid != 0 && self.lower_resting.position > EPSILON && lower_change {
            let cancel = self
                .attempt_cancel(self.asset.clone(), self.lower_resting.oid)
                .await;
            // If we were unable to cancel, it means we got a fill, so wait until we receive that event to do anything
            if !cancel {
                return;
            }
            info!("Cancelled buy order: {:?}", self.lower_resting);
        }

        if self.upper_resting.oid != 0 && self.upper_resting.position > EPSILON && upper_change {
            let cancel = self
                .attempt_cancel(self.asset.clone(), self.upper_resting.oid)
                .await;
            if !cancel {
                return;
            }
            info!("Cancelled sell order: {:?}", self.upper_resting);
        }

        // Consider putting a new order up
        if lower_order_amount > EPSILON && lower_change {
            let (amount_resting, oid) = self
                .place_order(self.asset.clone(), lower_order_amount, lower_price, true)
                .await;

            self.lower_resting.oid = oid;
            self.lower_resting.position = amount_resting;
            self.lower_resting.price = lower_price;

            if amount_resting > EPSILON {
                info!(
                    "Buy for {amount_resting} {} resting at {lower_price}",
                    self.asset
                );
            }
        }

        if upper_order_amount > EPSILON && upper_change {
            let (amount_resting, oid) = self
                .place_order(self.asset.clone(), upper_order_amount, upper_price, false)
                .await;
            self.upper_resting.oid = oid;
            self.upper_resting.position = amount_resting;
            self.upper_resting.price = upper_price;

            if amount_resting > EPSILON {
                info!(
                    "Sell for {amount_resting} {} resting at {upper_price}",
                    self.asset
                );
            }
        }
    }
}
