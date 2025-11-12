//! A generic device abstraction for mapping IR remote buttons to application-specific actions.
//!
//! See [`IrMapping`] for usage examples.

use embassy_executor::Spawner;
use embassy_rp::Peri;
use embassy_rp::gpio::Pin;
use heapless::LinearMap;

use crate::ir::{Ir, IrEvent, IrNotifier};
use crate::Result;

/// Notifier channel for IR mapping events.
/// 
/// See [`IrMapping`] for usage examples.
pub struct IrMappingNotifier(IrNotifier);

impl IrMappingNotifier {
    /// Create a new mapping notifier.
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self(Ir::notifier())
    }

    /// Get a reference to the inner notifier.
    #[must_use]
    pub(crate) const fn inner(&self) -> &IrNotifier {
        &self.0
    }
}

/// A generic device abstraction that maps IR remote button presses to user-defined button types.
///
/// # Examples
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// # use core::prelude::rust_2024::derive;
/// # use embassy_executor::Spawner;
/// # use serials::ir_mapping::{IrMapping, IrMappingNotifier};
/// #
/// #[derive(Debug, Clone, Copy)]
/// enum RemoteButton { Power, Play, Stop }
/// # async fn example(p: embassy_rp::Peripherals, spawner: Spawner) -> serials::Result<()> {
/// let button_map = [
///     (0x0000, 0x45, RemoteButton::Power),
///     (0x0000, 0x0C, RemoteButton::Play),
///     (0x0000, 0x08, RemoteButton::Stop),
/// ];
///
/// static IR_MAPPING_NOTIFIER: IrMappingNotifier = IrMapping::<RemoteButton, 3>::notifier();
/// let ir_mapping: IrMapping<RemoteButton, 3> = IrMapping::new(p.PIN_15, &button_map, &IR_MAPPING_NOTIFIER, spawner)?;
///
/// loop {
///     let button = ir_mapping.wait().await;
///     // Use button...
/// }
/// # }
/// ```
pub struct IrMapping<'a, B, const N: usize> {
    ir: Ir<'a>,
    button_map: LinearMap<(u16, u8), B, N>,
}

impl<'a, B, const N: usize> IrMapping<'a, B, N>
where
    B: Copy,
{
    /// Create a new notifier channel for IR events.
    ///
    /// See [`IrMapping`] for usage examples.
    #[must_use]
    pub const fn notifier() -> IrMappingNotifier {
        IrMappingNotifier::new()
    }

    /// Create a new IR remote button mapper.
    ///
    /// # Parameters
    /// - `pin`: GPIO pin connected to the IR receiver
    /// - `button_map`: Array mapping (address, command) pairs to button types
    /// - `notifier`: Static reference to the notifier channel
    /// - `spawner`: Embassy spawner for background task
    ///
    /// See [`IrMapping`] for usage examples.
    ///
    /// # Errors
    /// Returns an error if the background task cannot be spawned.
    pub fn new<P: Pin>(
        pin: Peri<'static, P>,
        button_map: &[(u16, u8, B)],
        notifier: &'static IrMappingNotifier,
        spawner: Spawner,
    ) -> Result<Self> {
        let ir = Ir::new(pin, notifier.inner(), spawner)?;
        
        // Convert the flat array to a LinearMap
        let mut map = LinearMap::new();
        for &(addr, cmd, button) in button_map {
            let _ = map.insert((addr, cmd), button);
        }
        
        Ok(Self { ir, button_map: map })
    }

    /// Wait for the next recognized button press.
    ///
    /// Ignores button presses that are not in the button map.
    ///
    /// See [`IrMapping`] for usage examples.
    pub async fn wait(&self) -> B {
        loop {
            let IrEvent::Press { addr, cmd } = self.ir.wait().await;
            #[cfg(feature = "defmt")]
            defmt::info!("IR received - addr=0x{:04X} cmd=0x{:02X}", addr, cmd);
            if let Some(&button) = self.button_map.get(&(addr, cmd)) {
                return button;
            }
            #[cfg(feature = "defmt")]
            defmt::info!("  (unrecognized - ignoring)");
        }
    }
}
