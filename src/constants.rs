pub const APPLICATION_JSON: &str = "application/json";

pub const CONTENT_TYPE: &str = "Content-Type";

pub const METADATA_QUERY: &str = "
  query { \
    _metadata \
    { \
      lastProcessedHeight \
      lastProcessedTimestamp \
      targetHeight \
      chain \
      specName \
      genesisHash \
      indexerHealthy \
      indexerNodeVersion \
      queryNodeVersion \
      indexerHealthy \
      chain \
    } \
  }";
