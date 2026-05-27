use lettre::Address;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, ops::Deref, str::FromStr};

use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MailAddress(Address);

impl From<Address> for MailAddress {
    fn from(address: Address) -> Self {
        MailAddress(address)
    }
}

impl From<MailAddress> for Address {
    fn from(mail_address: MailAddress) -> Self {
        mail_address.0
    }
}

impl Deref for MailAddress {
    type Target = Address;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for MailAddress {
    type Err = AppError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let address = s
            .parse()
            .map_err(|_| AppError::InvalidInput("Invalid email".to_string()))?;
        Ok(MailAddress(address))
    }
}

impl Display for MailAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for MailAddress {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_ref())
    }
}

impl<'de> Deserialize<'de> for MailAddress {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_str(MailAddressVisitor)
    }
}

struct MailAddressVisitor;

impl<'de> serde::de::Visitor<'de> for MailAddressVisitor {
    type Value = MailAddress;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "a valid email address")
    }

    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        let address = v
            .parse()
            .map_err(|_| E::invalid_value(serde::de::Unexpected::Str(v), &self))?;
        Ok(MailAddress(address))
    }
}
