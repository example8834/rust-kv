use std::vec::IntoIter;

use crate::{
    command_exchange::{CommandExchange, extract_bulk_string},
    error::{Command, EvalCommand, Frame, KvError, PingCommand, UnimplementCommand},
};

impl CommandExchange for PingCommand {
    fn exchange(
        mut itor: std::vec::IntoIter<Frame>,
        _command_name: String,
    ) -> Result<Command, KvError> {
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
        itor: std::vec::IntoIter<Frame>,
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

impl CommandExchange for EvalCommand {
    fn exchange(mut itor: IntoIter<Frame>, command_name: String) -> Result<Command, KvError> {
        //第一步 就是现实提取脚本字符串
        let script_frame = itor.next();
        let script = extract_bulk_string(script_frame)?;

        //第二个参数“必须”是 numkeys
        let numkeys_frame = itor.next();
        let numkeys_str = extract_bulk_string(numkeys_frame)?;
        let numkeys: usize = numkeys_str
            .parse()
            .map_err(|_| KvError::ProtocolError("numkeys must be an integer".into()))?;

        //`numkeys` 个参数“必须”是 keys
        let mut keys = Vec::with_capacity(numkeys);
        for _ in 0..numkeys {
            // 你用迭代器循环 `numkeys` 次
            let key_frame = itor
                .next()
                .ok_or(KvError::ProtocolError("Not enough keys provided".into()))?;
            keys.push(extract_bulk_string(key_frame.into())?);
        }

        // (固定位置 5) “剩下所有”的参数“必须”是 args
        // 你把迭代器“剩下”的所有东西都收集起来
        let args: Vec<String> = itor
            .map(|frame| extract_bulk_string(frame.into()))
            .collect::<Result<Vec<_>, _>>()?;

        // (封装) 你成功“封装”了 Command::Eval
        Ok(Command::EvalCommand(EvalCommand { script, keys, args }))
    }
}
