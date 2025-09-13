use crate::{
    command_exchange::{CommandExchange, extract_bulk_string},
    error::{Frame, KvError, PingCommand, UnimplementCommand},
};

impl CommandExchange for PingCommand {
    fn exchange(
        mut itor: std::vec::IntoIter<crate::error::Frame>,
        _command_name: String,
    ) -> Result<crate::error::Command, crate::error::KvError> {
        if itor.len() > 1 {
            return Err(KvError::ProtocolError("PING 命令最多只能有一个参数".into()));
        } else {
            if itor.len() == 0 {
                return Ok(crate::error::Command::Ping(PingCommand { value: None }));
            } else {
                let msg = extract_bulk_string(itor.next())?;
                Ok(crate::error::Command::Ping(PingCommand {
                    value: Some(msg),
                }))
            }
        }
    }
}

impl CommandExchange for UnimplementCommand {
    fn exchange(
        itor: std::vec::IntoIter<crate::error::Frame>,
        command_name: String,
    ) -> Result<crate::error::Command, crate::error::KvError> {
        let args = itor
            .skip(1)
            .map(|f| match f {
                Frame::Bulk(bytes) => Ok(bytes),
                _ => Err(KvError::ProtocolError("命令参数必须是 Bulk String".into())),
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(crate::error::Command::Unimplement(UnimplementCommand {
            command: command_name,
            args,
        }))
    }
}
