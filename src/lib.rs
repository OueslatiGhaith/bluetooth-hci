//! A Bluetooth implementation for embedded systems.
//!
//! This crate is a proof-of-concept implementation of the host (application) side of the
//! [`Bluetooth`] specification. It is still woefully incomplete, and will undoubtedly be redesigned
//! completely, and potentially split into multiple crates before being stabilized.
//!
//! When the documentation refers to a specific section of "the" Bluetooth specification, the same
//! section applies for all supported versions of the specification. If the versions differ, the
//! specific version will also be included in the reference.
//!
//! # Design
//!
//! Like other core embedded crates (e.g, [`embedded-hal`]), this crate uses traits to be agnostic
//! about the specific Bluetooth module. It provides a default implementation of the HCI for devices
//! that implement the core [`Controller`] trait. The traits also make use of async in traits, so the
//! #![feature(async_fn_in_trait)] feature is required.
//! support different asynchronous or synchronous operation modes.
//!
//! ## Commands
//!
//! The [`host::Hci`] trait defines all of the functions that communicate from the host to the
//! controller. The [`host::uart::Hci`] trait defines a read function that returns a
//! [`host::uart::Packet`], which can contain an [`Event`], `AclData` (TODO), or `SyncData`
//! (TODO). Both of these traits have default implementations in terms of the [`Controller`], so
//! calling code does not need to implement any commands or event parsing code.
//!
//! ## Vendor-specific commands and events
//!
//! The [`host::uart::Hci`] trait requires specialization for the type of vendor-specific events
//! (which implement [`event::VendorEvent`]) and vendor-specific errors. Any vendor-specific
//! extensions will need to convert byte buffers into the appropriate event type (as defined by the
//! vendor), but will not need to read data using the [`Controller`]. The Bluetooth standard
//! provides packet length in a common header, so only complete packets will be passed on to the
//! vendor code for deserialization.
//!
//! There is not yet support for vendor-specific commands. The vendor crate will have to serialize
//! the command packets directly and write them to the [`Controller`].
//!
//! # Reference implementation
//!
//! The [`bluenrg`] crate provides a sample implementation for STMicro's BlueNRG Bluetooth
//! controllers.
//!
//! # Ideas for discussion and improvement
//!
//! - Add traits to facilitate writing Bluetooth controllers. These controllers would have a host on
//!   one side and a link layer on the other. Separate crate? If so, move common definitions (Status
//!   codes, opcodes, etc.) to a bluetooth-core crate.
//!
//! - Add a helper function for vendor-specific commands. This should take care of creating the
//!   header and writing the data to the [`Controller`]. Vendor code should only be responsible for
//!   serializing commands into byte slices.
//!
//! - Remove the `cmd_link` and `event_link` modules, and merge `uart` up into `host`. The Bluetooth
//!   spec made it seem like there were devices that do not include the packet type byte at the
//!   beginning of packets, but STMicro's BlueNRG implementation and Nordic's Zephyr implementation
//!   both include it. If there is a controller that does *not* include the packet type, the
//!   `event_link` HCI can always be brought back.
//!
//! - Provide config features for different versions of the Bluetooth Specification.
//!
//! - Implement all of the specified functions and events.
//!
//! - Provide opt-in config features for certain types of commands and events. For example, BlueNRG
//!   devices only implement 40 commands and 14 events, but the spec has around 250 commands and 76
//!   events. It would be nice if unused events could be compiled out. This would be less important
//!   for commands, since those functions would simply never be called, and could be removed by the
//!   linker. This would entail significant work both on the part of the crate authors and on crate
//!   users, who would need to configure the crate appropriately. All combinations of features would
//!   also never be tested; there are simply too many, even if we only provide features for the
//!   events. On the other hand, those features should not interact, so maybe it would be feasible.
//!
//! [`Bluetooth`]: https://www.bluetooth.com/specifications/bluetooth-core-specification
//! [`embedded-hal`]: https://crates.io/crates/embedded-hal
//! [`bluenrg`]: https://github.com/danielgallagher0/bluenrg

#![no_std]
#![allow(async_fn_in_trait)]

extern crate byteorder;

// This must go FIRST so that all the other modules see its macros.
mod fmt;

#[macro_use]
pub mod bitflag_array;

pub mod event;
pub mod host;
mod opcode;
pub mod types;
pub mod vendor;

pub use event::Event;
pub use opcode::Opcode;

use core::fmt::Debug;

/// Interface to the Bluetooth controller from the host's perspective.
///
/// The Bluetooth application host must communicate with a controller (which, in turn, communicates
/// with the link layer) to control the Bluetooth radio. Device crates must implement this trait,
/// which enables full access to all of the functions and events of the HCI through [`host::Hci`]
/// and [`host::uart::Hci`], respectively.
pub trait Controller {
    /// Writes the bytes to the controller, in a single transaction if possible. All of `header`
    /// shall be written, followed by all of `payload`.
    async fn controller_write(&mut self, opcode: Opcode, payload: &[u8]);

    /// Reads data from the controller into the provided `buffer`. The length of the buffer
    /// indicates the number of bytes to read. The implementor must not return bytes in an order
    /// different from that in which they were received from the controller. For example, the
    /// implementor may read all available bytes from the controller and maintain them in an
    /// internal buffer, but `read_into` shall only read the number of bytes requested.
    ///
    /// # Example
    ///
    /// ```
    /// // Controller sends:
    /// // +------+------+------+------+------+------+------+------+
    /// // | 0x12 | 0x34 | 0x56 | 0x78 | 0x9a | 0xbc | 0xde | 0xf0 |
    /// // +------+------+------+------+------+------+------+------+
    ///
    /// // host calls:
    ///
    /// # extern crate stm32wb_hci as hci;
    /// # use hci::Controller as HciController;
    /// # struct Controller;
    /// # impl HciController for Controller {
    /// #     async fn controller_write(&mut self, opcode: hci::Opcode, _payload: &[u8]) {}
    /// #     async fn controller_read_into(&self, _buf: &mut [u8]) {}
    /// # }
    /// # fn main() {
    /// # let mut controller = Controller;
    /// let mut buffer = [0; 4];
    /// controller.controller_read_into(&mut buffer);
    ///
    /// // buffer contains:
    /// // +------+------+------+------+
    /// // | 0x00 | 0x12 | 0x34 | 0x56 |
    /// // +------+------+------+------+
    ///
    /// // now the host calls:
    /// controller.controller_read_into(&mut buffer);  // read 4 bytes into buffer
    ///
    /// // buffer contains:
    /// // +------+------+------+------+
    /// // | 0x78 | 0x9a | 0xbc | 0xde |
    /// // +------+------+------+------+
    /// # }
    /// ```
    async fn controller_read_into(&self, buf: &mut [u8]);
}

/// List of possible error codes, Bluetooth Spec, Vol 2, Part D, Section 2.
///
/// Includes an extension point for vendor-specific status codes.
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum Status {
    /// Success
    Success,
    /// Unknown HCI Command
    UnknownCommand,
    /// Unknown Connection Identifier
    UnknownConnectionId,
    /// Hardware Failure
    HardwareFailure,
    /// Page Timeout
    PageTimeout,
    /// Authentication Failure
    AuthFailure,
    /// PIN or Key Missing
    PinOrKeyMissing,
    /// Memory Capacity Exceeded
    OutOfMemory,
    /// Connection Timeout
    ConnectionTimeout,
    /// Connection Limit Exceeded
    ConnectionLimitExceeeded,
    /// Synchronous Connection Limit To A Device Exceeded
    SyncConnectionLimitExceeded,
    /// Connection Already Exists
    ConnectionAlreadyExists,
    /// Command Disallowed
    CommandDisallowed,
    /// Connection Rejected due to Limited Resources
    LimitedResources,
    /// Connection Rejected Due To Security Reasons
    ConnectionRejectedSecurity,
    /// Connection Rejected due to Unacceptable BD_ADDR
    UnacceptableBdAddr,
    /// Connection Accept Timeout Exceeded
    AcceptTimeoutExceeded,
    /// Unsupported Feature or Parameter Value
    UnsupportedFeature,
    /// Invalid HCI Command Parameters
    InvalidParameters,
    /// Remote User Terminated Connection
    RemoteTerminationByUser,
    /// Remote Device Terminated Connection due to Low Resources
    RemoteTerminationLowResources,
    /// Remote Device Terminated Connection due to Power Off
    RemoteTerminationPowerOff,
    /// Connection Terminated By Local Host
    ConnectionTerminatedByHost,
    /// Repeated Attempts
    RepeatedAttempts,
    /// Pairing Not Allowed
    PairingNotAllowed,
    /// Unknown LMP PDU
    UnknownLmpPdu,
    /// Unsupported Remote Feature / Unsupported LMP Feature
    UnsupportedRemoteFeature,
    /// SCO Offset Rejected
    ScoOffsetRejected,
    /// SCO Interval Rejected
    ScoIntervalRejected,
    /// SCO Air Mode Rejected
    ScoAirModeRejected,
    /// Invalid LMP Parameters / Invalid LL Parameters
    InvalidLmpParameters,
    /// Unspecified Error
    UnspecifiedError,
    /// Unsupported LMP Parameter Value / Unsupported LL Parameter Value
    UnsupportedLmpParameterValue,
    /// Role Change Not Allowed
    RoleChangeNotAllowed,
    /// LMP Response Timeout / LL Response Timeout
    LmpResponseTimeout,
    /// LMP Error Transaction Collision / LL Procedure Collision
    LmpTransactionCollision,
    /// LMP PDU Not Allowed
    LmpPduNotAllowed,
    /// Encryption Mode Not Acceptable
    EncryptionModeNotAcceptable,
    /// Link Key cannot be Changed
    LinkKeyCannotBeChanged,
    /// Requested QoS Not Supported
    RequestedQosNotSupported,
    /// Instant Passed
    InstantPassed,
    /// Pairing With Unit Key Not Supported
    PairingWithUnitKeyNotSupported,
    /// Different Transaction Collision
    DifferentTransactionCollision,
    /// Reserved for Future Use
    ReservedforFutureUse,
    /// QoS Unacceptable Parameter
    QosUnacceptableParameter,
    /// QoS Rejected
    QosRejected,
    /// Channel Classification Not Supported
    ChannelClassificationNotSupported,
    /// Insufficient Security
    InsufficientSecurity,
    /// Parameter Out Of Mandatory Range
    ParameterOutOfMandatoryRange,
    /// Reserved for Future Use
    ReservedForFutureUse49,
    /// Role Switch Pending
    RoleSwitchPending,
    /// Reserved for Future Use
    ReservedForFutureUse51,
    /// Reserved Slot Violation
    ReservedSlotViolation,
    /// Role Switch Failed
    RoleSwitchFailed,
    /// Extended Inquiry Response Too Large
    ExtendedInquiryResponseTooLarge,
    /// Secure Simple Pairing Not Supported By Host
    SecureSimplePairingNotSupportedByHost,
    /// Host Busy - Pairing
    HostBusyPairing,
    /// Connection Rejected due to No Suitable Channel Found
    ConnectionRejectedNoSuitableChannel,
    /// Controller Busy
    ControllerBusy,
    /// Unacceptable Connection Parameters
    UnacceptableConnectionParameters,
    /// Advertising Timeout
    AdvertisingTimeout,
    /// Connection Terminated due to MIC Failure
    ConnectionTerminatedMicFailure,
    /// Connection Failed to be Established
    ConnectionFailedToEstablish,
    /// MAC Connection Failed
    MacConnectionFailed,
    /// Coarse Clock Adjustment Rejected but Will Try to Adjust Using Clock Dragging
    CoarseClockAdjustmentRejectedDraggingAttempted,
    /// Type0 Submap Not Defined
    ///
    /// First introduced in version 5.0
    Type0SubmapNotDefined,
    /// Unknown Advertising Identifier
    ///
    /// First introduced in version 5.0
    UnknownAdvertisingId,
    /// Limit Reached
    ///
    /// First introduced in version 5.0
    LimitReached,
    /// Operation Cancelled by Host
    ///
    /// First introduced in version 5.0
    OperationCancelledByHost,
    /// Vendor-specific status code
    Vendor(crate::vendor::event::VendorStatus),
}

/// Wrapper enum for errors converting a u8 into a [`Status`].
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BadStatusError {
    /// The value does not map to a [`Status`].
    BadValue(u8),
}

impl core::convert::TryFrom<u8> for Status {
    type Error = crate::BadStatusError;

    fn try_from(value: u8) -> Result<Status, Self::Error> {
        match value {
            0x00 => Ok(Status::Success),
            0x01 => Ok(Status::UnknownCommand),
            0x02 => Ok(Status::UnknownConnectionId),
            0x03 => Ok(Status::HardwareFailure),
            0x04 => Ok(Status::PageTimeout),
            0x05 => Ok(Status::AuthFailure),
            0x06 => Ok(Status::PinOrKeyMissing),
            0x07 => Ok(Status::OutOfMemory),
            0x08 => Ok(Status::ConnectionTimeout),
            0x09 => Ok(Status::ConnectionLimitExceeeded),
            0x0A => Ok(Status::SyncConnectionLimitExceeded),
            0x0B => Ok(Status::ConnectionAlreadyExists),
            0x0C => Ok(Status::CommandDisallowed),
            0x0D => Ok(Status::LimitedResources),
            0x0E => Ok(Status::ConnectionRejectedSecurity),
            0x0F => Ok(Status::UnacceptableBdAddr),
            0x10 => Ok(Status::AcceptTimeoutExceeded),
            0x11 => Ok(Status::UnsupportedFeature),
            0x12 => Ok(Status::InvalidParameters),
            0x13 => Ok(Status::RemoteTerminationByUser),
            0x14 => Ok(Status::RemoteTerminationLowResources),
            0x15 => Ok(Status::RemoteTerminationPowerOff),
            0x16 => Ok(Status::ConnectionTerminatedByHost),
            0x17 => Ok(Status::RepeatedAttempts),
            0x18 => Ok(Status::PairingNotAllowed),
            0x19 => Ok(Status::UnknownLmpPdu),
            0x1A => Ok(Status::UnsupportedRemoteFeature),
            0x1B => Ok(Status::ScoOffsetRejected),
            0x1C => Ok(Status::ScoIntervalRejected),
            0x1D => Ok(Status::ScoAirModeRejected),
            0x1E => Ok(Status::InvalidLmpParameters),
            0x1F => Ok(Status::UnspecifiedError),
            0x20 => Ok(Status::UnsupportedLmpParameterValue),
            0x21 => Ok(Status::RoleChangeNotAllowed),
            0x22 => Ok(Status::LmpResponseTimeout),
            0x23 => Ok(Status::LmpTransactionCollision),
            0x24 => Ok(Status::LmpPduNotAllowed),
            0x25 => Ok(Status::EncryptionModeNotAcceptable),
            0x26 => Ok(Status::LinkKeyCannotBeChanged),
            0x27 => Ok(Status::RequestedQosNotSupported),
            0x28 => Ok(Status::InstantPassed),
            0x29 => Ok(Status::PairingWithUnitKeyNotSupported),
            0x2A => Ok(Status::DifferentTransactionCollision),
            0x2B => Ok(Status::ReservedforFutureUse),
            0x2C => Ok(Status::QosUnacceptableParameter),
            0x2D => Ok(Status::QosRejected),
            0x2E => Ok(Status::ChannelClassificationNotSupported),
            0x2F => Ok(Status::InsufficientSecurity),
            0x30 => Ok(Status::ParameterOutOfMandatoryRange),
            0x31 => Ok(Status::ReservedForFutureUse49),
            0x32 => Ok(Status::RoleSwitchPending),
            0x33 => Ok(Status::ReservedForFutureUse51),
            0x34 => Ok(Status::ReservedSlotViolation),
            0x35 => Ok(Status::RoleSwitchFailed),
            0x36 => Ok(Status::ExtendedInquiryResponseTooLarge),
            0x37 => Ok(Status::SecureSimplePairingNotSupportedByHost),
            0x38 => Ok(Status::HostBusyPairing),
            0x39 => Ok(Status::ConnectionRejectedNoSuitableChannel),
            0x3A => Ok(Status::ControllerBusy),
            0x3B => Ok(Status::UnacceptableConnectionParameters),
            0x3C => Ok(Status::AdvertisingTimeout),
            0x3D => Ok(Status::ConnectionTerminatedMicFailure),
            0x3E => Ok(Status::ConnectionFailedToEstablish),
            0x3F => Ok(Status::MacConnectionFailed),
            0x40 => Ok(Status::CoarseClockAdjustmentRejectedDraggingAttempted),
            0x41 => Ok(Status::Type0SubmapNotDefined),
            0x42 => Ok(Status::UnknownAdvertisingId),
            0x43 => Ok(Status::LimitReached),
            0x44 => Ok(Status::OperationCancelledByHost),
            _ => Ok(Status::Vendor(
                crate::vendor::event::VendorStatus::try_from(value)?,
            )),
        }
    }
}

impl core::convert::From<Status> for u8 {
    fn from(val: Status) -> Self {
        match val {
            Status::Success => 0x00,
            Status::UnknownCommand => 0x01,
            Status::UnknownConnectionId => 0x02,
            Status::HardwareFailure => 0x03,
            Status::PageTimeout => 0x04,
            Status::AuthFailure => 0x05,
            Status::PinOrKeyMissing => 0x06,
            Status::OutOfMemory => 0x07,
            Status::ConnectionTimeout => 0x08,
            Status::ConnectionLimitExceeeded => 0x09,
            Status::SyncConnectionLimitExceeded => 0x0A,
            Status::ConnectionAlreadyExists => 0x0B,
            Status::CommandDisallowed => 0x0C,
            Status::LimitedResources => 0x0D,
            Status::ConnectionRejectedSecurity => 0x0E,
            Status::UnacceptableBdAddr => 0x0F,
            Status::AcceptTimeoutExceeded => 0x10,
            Status::UnsupportedFeature => 0x11,
            Status::InvalidParameters => 0x12,
            Status::RemoteTerminationByUser => 0x13,
            Status::RemoteTerminationLowResources => 0x14,
            Status::RemoteTerminationPowerOff => 0x15,
            Status::ConnectionTerminatedByHost => 0x16,
            Status::RepeatedAttempts => 0x17,
            Status::PairingNotAllowed => 0x18,
            Status::UnknownLmpPdu => 0x19,
            Status::UnsupportedRemoteFeature => 0x1A,
            Status::ScoOffsetRejected => 0x1B,
            Status::ScoIntervalRejected => 0x1C,
            Status::ScoAirModeRejected => 0x1D,
            Status::InvalidLmpParameters => 0x1E,
            Status::UnspecifiedError => 0x1F,
            Status::UnsupportedLmpParameterValue => 0x20,
            Status::RoleChangeNotAllowed => 0x21,
            Status::LmpResponseTimeout => 0x22,
            Status::LmpTransactionCollision => 0x23,
            Status::LmpPduNotAllowed => 0x24,
            Status::EncryptionModeNotAcceptable => 0x25,
            Status::LinkKeyCannotBeChanged => 0x26,
            Status::RequestedQosNotSupported => 0x27,
            Status::InstantPassed => 0x28,
            Status::PairingWithUnitKeyNotSupported => 0x29,
            Status::DifferentTransactionCollision => 0x2A,
            Status::ReservedforFutureUse => 0x2B,
            Status::QosUnacceptableParameter => 0x2C,
            Status::QosRejected => 0x2D,
            Status::ChannelClassificationNotSupported => 0x2E,
            Status::InsufficientSecurity => 0x2F,
            Status::ParameterOutOfMandatoryRange => 0x30,
            Status::ReservedForFutureUse49 => 0x31,
            Status::RoleSwitchPending => 0x32,
            Status::ReservedForFutureUse51 => 0x33,
            Status::ReservedSlotViolation => 0x34,
            Status::RoleSwitchFailed => 0x35,
            Status::ExtendedInquiryResponseTooLarge => 0x36,
            Status::SecureSimplePairingNotSupportedByHost => 0x37,
            Status::HostBusyPairing => 0x38,
            Status::ConnectionRejectedNoSuitableChannel => 0x39,
            Status::ControllerBusy => 0x3A,
            Status::UnacceptableConnectionParameters => 0x3B,
            Status::AdvertisingTimeout => 0x3C,
            Status::ConnectionTerminatedMicFailure => 0x3D,
            Status::ConnectionFailedToEstablish => 0x3E,
            Status::MacConnectionFailed => 0x3F,
            Status::CoarseClockAdjustmentRejectedDraggingAttempted => 0x40,
            _ => match val {
                Status::Type0SubmapNotDefined => 0x41,
                Status::UnknownAdvertisingId => 0x42,
                Status::LimitReached => 0x43,
                Status::OperationCancelledByHost => 0x44,
                Status::Vendor(v) => v.into(),
                _ => 0xFF,
            },
        }
    }
}

/// Newtype for a connection handle.
///
/// Values:
/// - 0x0000 .. 0xEFFF: Unenhanced ATT bearer
/// - 0xEA00 .. 0xEA3F: Enhanced ATT bearer (the LSB-byte of the parameter is
/// the connection oriented channel index)
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct ConnectionHandle(pub u16);

/// Newtype for BDADDR.
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub struct BdAddr(pub [u8; 6]);

/// Potential values for BDADDR
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
pub enum BdAddrType {
    /// Public address.
    Public(BdAddr),

    /// Random address.
    Random(BdAddr),
}

impl BdAddrType {
    /// Writes a `BdAddrType` into the given slice.  The slice must be exactly the right length (7
    /// bytes).
    pub fn copy_into_slice(&self, bytes: &mut [u8]) {
        assert_eq!(bytes.len(), 7);
        match *self {
            BdAddrType::Public(addr) => {
                bytes[0] = 0;
                bytes[1..7].copy_from_slice(&addr.0);
            }
            BdAddrType::Random(addr) => {
                bytes[0] = 1;
                bytes[1..7].copy_from_slice(&addr.0);
            }
        }
    }
}

/// The BD Address type is not recognized.  Includes the unrecognized byte.
///
/// See [`to_bd_addr_type`]
pub struct BdAddrTypeError(pub u8);

/// Wraps a [`BdAddr`] in a [`BdAddrType`].
///
/// # Errors
///
/// - `bd_addr_type` does not denote an appropriate type. Returns the byte. The address is
///   discarded.
pub fn to_bd_addr_type(bd_addr_type: u8, addr: BdAddr) -> Result<BdAddrType, BdAddrTypeError> {
    match bd_addr_type {
        0 => Ok(BdAddrType::Public(addr)),
        1 => Ok(BdAddrType::Random(addr)),
        _ => Err(BdAddrTypeError(bd_addr_type)),
    }
}

#[cfg(not(feature = "defmt"))]
bitflags::bitflags! {
    /// Bitfield for LE Remote Features.
    ///
    /// Fields are defined in Vol 6, Part B, Section 4.6 of the spec.  See Table 4.3 (version 4.1)
    /// or Table 4.4 (version 4.2 and 5.0).
    #[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
    pub struct LinkLayerFeature : u64 {
        /// See section 4.6.1
        const LE_ENCRYPTION = 1 << 0;
        /// See section 4.6.2
        const CONNECTION_PARAMETERS_REQUEST_PROCEDURE = 1 << 1;
        /// See section 4.6.3
        const EXTENDED_REJECT_INDICATION = 1 << 2;
        /// See section 4.6.4
        const PERIPHERAL_INITIATED_FEATURES_EXCHANGE = 1 << 3;
        /// See section 4.6.5
        const LE_PING = 1 << 4;
        /// See section 4.6.6
        const LE_DATA_PACKET_LENGTH_EXTENSION = 1 << 5;
        /// See section 4.6.7
        const LL_PRIVACY = 1 << 6;
        /// See section 4.6.8
        const EXTENDED_SCANNER_FILTER_POLICIES = 1 << 7;
        /// See section 4.6.9
        const LE_2M_PHY = 1 << 8;
        /// See section 4.6.10
        const STABLE_MODULATION_INDEX_TX = 1 << 9;
        /// See section 4.6.11
        const STABLE_MODULATION_INDEX_RX = 1 << 10;
        /// Not in section 4.6
        const LE_CODED_PHY = 1 << 11;
        /// See section 4.6.12
        const LE_EXTENDED_ADVERTISING = 1 << 12;
        /// See section 4.6.13
        const LE_PERIODIC_ADVERTISING = 1 << 13;
        /// See section 4.6.14
        const CHANNEL_SELECTION_ALGORITHM_2 = 1 << 14;
        /// Not in section 4.6
        const LE_POWER_CLASS_1 = 1 << 15;
        /// See section 4.6.15
        const MINIMUM_NUMBER_OF_USED_CHANNELS_PROCEDURE = 1 << 16;
    }
}

#[cfg(feature = "defmt")]
defmt::bitflags! {
    /// Bitfield for LE Remote Features.
    ///
    /// Fields are defined in Vol 6, Part B, Section 4.6 of the spec.  See Table 4.3 (version 4.1)
    /// or Table 4.4 (version 4.2 and 5.0).
    #[derive(Default)]
    pub struct LinkLayerFeature : u64 {
        /// See section 4.6.1
        const LE_ENCRYPTION = 1 << 0;
        /// See section 4.6.2
        const CONNECTION_PARAMETERS_REQUEST_PROCEDURE = 1 << 1;
        /// See section 4.6.3
        const EXTENDED_REJECT_INDICATION = 1 << 2;
        /// See section 4.6.4
        const PERIPHERAL_INITIATED_FEATURES_EXCHANGE = 1 << 3;
        /// See section 4.6.5
        const LE_PING = 1 << 4;
        /// See section 4.6.6
        const LE_DATA_PACKET_LENGTH_EXTENSION = 1 << 5;
        /// See section 4.6.7
        const LL_PRIVACY = 1 << 6;
        /// See section 4.6.8
        const EXTENDED_SCANNER_FILTER_POLICIES = 1 << 7;
        /// See section 4.6.9
        const LE_2M_PHY = 1 << 8;
        /// See section 4.6.10
        const STABLE_MODULATION_INDEX_TX = 1 << 9;
        /// See section 4.6.11
        const STABLE_MODULATION_INDEX_RX = 1 << 10;
        /// Not in section 4.6
        const LE_CODED_PHY = 1 << 11;
        /// See section 4.6.12
        const LE_EXTENDED_ADVERTISING = 1 << 12;
        /// See section 4.6.13
        const LE_PERIODIC_ADVERTISING = 1 << 13;
        /// See section 4.6.14
        const CHANNEL_SELECTION_ALGORITHM_2 = 1 << 14;
        /// Not in section 4.6
        const LE_POWER_CLASS_1 = 1 << 15;
        /// See section 4.6.15
        const MINIMUM_NUMBER_OF_USED_CHANNELS_PROCEDURE = 1 << 16;
    }
}

bitflag_array! {
    /// Channel classifications for the LE Set Host Channel Classification command.
    ///
    /// If a flag is set, its classification is "Unknown".  If the flag is cleared, it is known
    /// "bad".
    #[derive(Copy, Clone, Debug)]
    #[cfg_attr(feature = "defmt", derive(defmt::Format))]
    pub struct ChannelClassification : 5;
    pub struct ChannelFlag;

    /// Channel 0 classification not known.
    const CH_0 = 0, 1 << 0;
    /// Channel 1 classification not known.
    const CH_1 = 0, 1 << 1;
    /// Channel 2 classification not known.
    const CH_2 = 0, 1 << 2;
    /// Channel 3 classification not known.
    const CH_3 = 0, 1 << 3;
    /// Channel 4 classification not known.
    const CH_4 = 0, 1 << 4;
    /// Channel 5 classification not known.
    const CH_5 = 0, 1 << 5;
    /// Channel 6 classification not known.
    const CH_6 = 0, 1 << 6;
    /// Channel 7 classification not known.
    const CH_7 = 0, 1 << 7;
    /// Channel 8 classification not known.
    const CH_8 = 1, 1 << 0;
    /// Channel 9 classification not known.
    const CH_9 = 1, 1 << 1;
    /// Channel 10 classification not known.
    const CH_10 = 1, 1 << 2;
    /// Channel 11 classification not known.
    const CH_11 = 1, 1 << 3;
    /// Channel 12 classification not known.
    const CH_12 = 1, 1 << 4;
    /// Channel 13 classification not known.
    const CH_13 = 1, 1 << 5;
    /// Channel 14 classification not known.
    const CH_14 = 1, 1 << 6;
    /// Channel 15 classification not known.
    const CH_15 = 1, 1 << 7;
    /// Channel 16 classification not known.
    const CH_16 = 2, 1 << 0;
    /// Channel 17 classification not known.
    const CH_17 = 2, 1 << 1;
    /// Channel 18 classification not known.
    const CH_18 = 2, 1 << 2;
    /// Channel 19 classification not known.
    const CH_19 = 2, 1 << 3;
    /// Channel 20 classification not known.
    const CH_20 = 2, 1 << 4;
    /// Channel 21 classification not known.
    const CH_21 = 2, 1 << 5;
    /// Channel 22 classification not known.
    const CH_22 = 2, 1 << 6;
    /// Channel 23 classification not known.
    const CH_23 = 2, 1 << 7;
    /// Channel 24 classification not known.
    const CH_24 = 3, 1 << 0;
    /// Channel 25 classification not known.
    const CH_25 = 3, 1 << 1;
    /// Channel 26 classification not known.
    const CH_26 = 3, 1 << 2;
    /// Channel 27 classification not known.
    const CH_27 = 3, 1 << 3;
    /// Channel 28 classification not known.
    const CH_28 = 3, 1 << 4;
    /// Channel 29 classification not known.
    const CH_29 = 3, 1 << 5;
    /// Channel 30 classification not known.
    const CH_30 = 3, 1 << 6;
    /// Channel 31 classification not known.
    const CH_31 = 3, 1 << 7;
    /// Channel 32 classification not known.
    const CH_32 = 4, 1 << 0;
    /// Channel 33 classification not known.
    const CH_33 = 4, 1 << 1;
    /// Channel 34 classification not known.
    const CH_34 = 4, 1 << 2;
    /// Channel 35 classification not known.
    const CH_35 = 4, 1 << 3;
    /// Channel 36 classification not known.
    const CH_36 = 4, 1 << 4;
}
