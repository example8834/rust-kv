use tokio::sync::mpsc::Sender;

use crate::{core_aof::AofMessage, db::Db, error::{Frame, KvError}};
mod string;

pub struct CommandContext<'a> {
    pub db: &'a Db,
    pub tx: &'a Option<Sender<AofMessage>>
}


pub trait CommandExecutor {
    // execute 方法現在接收 CommandContext 作為參數！
    async fn execute(self, ctx: &mut CommandContext) -> Result<Frame, KvError>;
}
