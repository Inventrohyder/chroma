use crate::{
    execution::operator::Operator,
    log::log::{Log, PullLogsError},
    types::EmbeddingRecord,
};
use async_trait::async_trait;
use uuid::Uuid;

/// The pull logs operator is responsible for reading logs from the log service.
#[derive(Debug)]
pub struct PullLogsOperator {
    client: Box<dyn Log>,
}

impl PullLogsOperator {
    /// Create a new pull logs operator.
    /// # Parameters
    /// * `client` - The log client to use for reading logs.
    pub fn new(client: Box<dyn Log>) -> Box<Self> {
        Box::new(PullLogsOperator { client })
    }
}

/// The input to the pull logs operator.
/// # Parameters
/// * `collection_id` - The collection id to read logs from.
/// * `offset` - The offset to start reading logs from.
/// * `batch_size` - The number of log entries to read.
/// * `num_records` - The maximum number of records to read.
/// * `end_timestamp` - The end timestamp to read logs until.
#[derive(Debug)]
pub struct PullLogsInput {
    collection_id: Uuid,
    offset: i64,
    batch_size: i32,
    num_records: Option<i32>,
    end_timestamp: Option<i64>,
}

impl PullLogsInput {
    /// Create a new pull logs input.
    /// # Parameters
    /// * `collection_id` - The collection id to read logs from.
    /// * `offset` - The offset to start reading logs from.
    /// * `batch_size` - The number of log entries to read.
    /// * `num_records` - The maximum number of records to read.
    /// * `end_timestamp` - The end timestamp to read logs until.
    pub fn new(
        collection_id: Uuid,
        offset: i64,
        batch_size: i32,
        num_records: Option<i32>,
        end_timestamp: Option<i64>,
    ) -> Self {
        PullLogsInput {
            collection_id,
            offset,
            batch_size,
            num_records,
            end_timestamp,
        }
    }
}

/// The output of the pull logs operator.
#[derive(Debug)]
pub struct PullLogsOutput {
    logs: Vec<Box<EmbeddingRecord>>,
}

impl PullLogsOutput {
    /// Create a new pull logs output.
    /// # Parameters
    /// * `logs` - The logs that were read.
    pub fn new(logs: Vec<Box<EmbeddingRecord>>) -> Self {
        PullLogsOutput { logs }
    }

    /// Get the log entries that were read by an invocation of the pull logs operator.
    /// # Returns
    /// The log entries that were read.
    pub fn logs(&self) -> &Vec<Box<EmbeddingRecord>> {
        &self.logs
    }
}

pub type PullLogsResult = Result<PullLogsOutput, PullLogsError>;

#[async_trait]
impl Operator<PullLogsInput, PullLogsOutput> for PullLogsOperator {
    type Error = PullLogsError;

    async fn run(&self, input: &PullLogsInput) -> PullLogsResult {
        // We expect the log to be cheaply cloneable, we need to clone it since we need
        // a mutable reference to it. Not necessarily the best, but it works for our needs.
        let mut client_clone = self.client.clone();
        let batch_size = input.batch_size;
        let mut num_records_read = 0;
        let mut offset = input.offset;
        let mut result = Vec::new();
        loop {
            let logs = client_clone
                .read(
                    input.collection_id.to_string(),
                    offset,
                    batch_size,
                    input.end_timestamp,
                )
                .await;

            let mut logs = match logs {
                Ok(logs) => logs,
                Err(e) => {
                    return Err(e);
                }
            };

            if logs.is_empty() {
                break;
            }

            num_records_read += logs.len();
            offset += batch_size as i64;
            result.append(&mut logs);

            if input.num_records.is_some()
                && num_records_read >= input.num_records.unwrap() as usize
            {
                break;
            }
        }
        if input.num_records.is_some() && result.len() > input.num_records.unwrap() as usize {
            result.truncate(input.num_records.unwrap() as usize);
        }
        Ok(PullLogsOutput::new(result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::log::log::InMemoryLog;
    use crate::log::log::LogRecord;
    use crate::types::EmbeddingRecord;
    use crate::types::Operation;
    use num_bigint::BigInt;
    use std::str::FromStr;
    use uuid::Uuid;

    #[tokio::test]
    async fn test_pull_logs() {
        let mut log = Box::new(InMemoryLog::new());

        let collection_uuid_1 = Uuid::from_str("00000000-0000-0000-0000-000000000001").unwrap();
        let collection_id_1 = collection_uuid_1.to_string();
        log.add_log(
            collection_id_1.clone(),
            Box::new(LogRecord {
                collection_id: collection_id_1.clone(),
                log_id: 1,
                log_id_ts: 1,
                record: Box::new(EmbeddingRecord {
                    id: "embedding_id_1".to_string(),
                    seq_id: BigInt::from(1),
                    embedding: None,
                    encoding: None,
                    metadata: None,
                    operation: Operation::Add,
                    collection_id: collection_uuid_1,
                }),
            }),
        );
        log.add_log(
            collection_id_1.clone(),
            Box::new(LogRecord {
                collection_id: collection_id_1.clone(),
                log_id: 2,
                log_id_ts: 2,
                record: Box::new(EmbeddingRecord {
                    id: "embedding_id_2".to_string(),
                    seq_id: BigInt::from(2),
                    embedding: None,
                    encoding: None,
                    metadata: None,
                    operation: Operation::Add,
                    collection_id: collection_uuid_1,
                }),
            }),
        );

        let operator = PullLogsOperator::new(log);

        // Pull all logs from collection 1
        let input = PullLogsInput::new(collection_uuid_1, 0, 1, None, None);
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 2);

        // Pull all logs from collection 1 with a large batch size
        let input = PullLogsInput::new(collection_uuid_1, 0, 100, None, None);
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 2);

        // Pull logs from collection 1 with a limit
        let input = PullLogsInput::new(collection_uuid_1, 0, 1, Some(1), None);
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 1);

        // Pull logs from collection 1 with an end timestamp
        let input = PullLogsInput::new(collection_uuid_1, 0, 1, None, Some(1));
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 1);

        // Pull logs from collection 1 with an end timestamp
        let input = PullLogsInput::new(collection_uuid_1, 0, 1, None, Some(2));
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 2);

        // Pull logs from collection 1 with an end timestamp and a limit
        let input = PullLogsInput::new(collection_uuid_1, 0, 1, Some(1), Some(2));
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 1);

        // Pull logs from collection 1 with a limit and a large batch size
        let input = PullLogsInput::new(collection_uuid_1, 0, 100, Some(1), None);
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 1);

        // Pull logs from collection 1 with an end timestamp and a large batch size
        let input = PullLogsInput::new(collection_uuid_1, 0, 100, None, Some(1));
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 1);

        // Pull logs from collection 1 with an end timestamp and a large batch size
        let input = PullLogsInput::new(collection_uuid_1, 0, 100, None, Some(2));
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 2);

        // Pull logs from collection 1 with an end timestamp and a limit and a large batch size
        let input = PullLogsInput::new(collection_uuid_1, 0, 100, Some(1), Some(2));
        let output = operator.run(&input).await.unwrap();
        assert_eq!(output.logs().len(), 1);
    }
}
