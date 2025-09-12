use std::vec::IntoIter;

use crate::{command_exchange::{extract_bulk_bytes, extract_bulk_integer, extract_bulk_string, CommandExchange}, error::{Command, Expiration, Frame, GetCommand, KvError, SetCommand, SetCondition}};

impl CommandExchange for SetCommand {
     fn exchange( mut itor: IntoIter<Frame>) -> Result<Command, KvError> {
        let key = extract_bulk_string(itor.next())?;
        let value = extract_bulk_bytes(itor.next())?;
        let mut expiration: Option<Expiration> = None;
        let mut condition: Option<SetCondition> = None;
        while let Some(frame) = itor.next() {
            match frame {
                Frame::Bulk(ref bytes) if bytes.eq_ignore_ascii_case(b"PX") => {
                    if let Some(time_frame) = itor.next() {
                        let time = extract_bulk_integer(Some(time_frame))?;
                        if time <= 0 {
                            return Err(KvError::ProtocolError("PX 过期时间必须大于 0".into()));
                        }
                        expiration = Some(Expiration::PX(time as u64));
                    } else {
                        return Err(KvError::ProtocolError("PX 需要一个时间参数".into()));
                    }
                }
                Frame::Bulk(ref bytes) if bytes.eq_ignore_ascii_case(b"EX") => {
                    if let Some(time_frame) = itor.next() {
                        let time = extract_bulk_integer(Some(time_frame))?;
                        if time <= 0 {
                            return Err(KvError::ProtocolError("EX 过期时间必须大于 0".into()));
                        }
                        expiration = Some(Expiration::EX(time as u64));
                    } else {
                        return Err(KvError::ProtocolError("EX 需要一个时间参数".into()));
                    }
                }
                Frame::Bulk(ref bytes) if bytes.eq_ignore_ascii_case(b"NX") => {
                    if condition.is_some() {
                        return Err(KvError::ProtocolError("只能指定 NX 或 XX 中的一个".into()));
                    }
                    condition = Some(SetCondition::NX);
                }
                Frame::Bulk(ref bytes) if bytes.eq_ignore_ascii_case(b"XX") => {
                    if condition.is_some() {
                        return Err(KvError::ProtocolError("只能指定 NX 或 XX 中的一个".into()));
                    }
                    condition = Some(SetCondition::XX);
                }
                _ => {
                    return Err(KvError::ProtocolError("未知的参数".into()));
                }
            }
        }
        Ok(Command::Set(SetCommand {
            key,
            value,
            expiration,
            condition,
        }))
    }
}

impl CommandExchange for GetCommand {
    fn exchange(mut itor: IntoIter<Frame>) -> Result<Command, KvError> {
        let key = extract_bulk_string(itor.next())?;
        Ok(Command::Get(GetCommand { key }))
    }
    
}