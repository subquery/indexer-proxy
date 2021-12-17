# Indexer Proxy

## Run Locally

### Start `coordinator service`

- `yarn start:db`
- `yarn start:testnet`
- `yarn mock:projects`: add and start mock projects

### Start `proxy server`

- `cargo build`
- `./target/debug/indexer-proxy --secret-key your-key --service-url http://127.0.0.1:8000/graphql`

### Output help menu

```sh
./target/debug/indexer-proxy --help
Indexer Proxy 0.1.0
Command line for starting indexer proxy server

USAGE:
    indexer-proxy [OPTIONS] --secret-key <secret-key> --service-url <service-url>

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -p, --port <port>                  Port the service will listen on [default: 8003]
        --secret-key <secret-key>      Secret key for generating auth token
        --service-url <service-url>    Coordinator service endpoint
```

## APIs

### `/token?user_id=${user_id}&deployment_id=${id}`

```sh
curl -i -X GET http://127.0.0.1:8003/token?user_id="0x59ce189fd40611162017deb88d826C3485f41e0D"&deployment_id="QmdehxWNdDBNM5jXmb4qbCkGBpTW6ooVy17NYGoMEeowxt"
```

Response:

```json
{ 
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzUxMiJ9.eyJ1c2VyIjp7InVzZXJfaWQiOiIweDU5Y2UxODlmZDQwNjExMTYyMDE3ZGViODhkODI2QzM0ODVmNDFlMEQiLCJkZXBsb3ltZW50X2lkIjoiMHg2YzgyMTI0MDhjM2M2MmZjNzhjYmZhOWQ2ZmU1ZmYzOTM0OGMxMDA5MTE0YTYzMTViMWUyMjU2NDU5MTM1MzQ4In0sImV4cCI6MTYzODg0MjA5MH0.4ej2RiEIPvSfKXisKCH2OYvu8WuLKMgKL59KlwpX6XTVUl0h57e63bdJjxxb109JwAGqkCVufKgj8m4OVETiyA"
}
```

### `/query/${deployment_id}`

#### Normal Query

```sh
export TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzUxMiJ9.eyJ1c2VyIjp7InVzZXJfaWQiOiIweGVlcmZzZGZkc2YiLCJkZXBsb3ltZW50X2lkIjoiMHg2YzgyMTI0MDhjM2M2MmZjNzhjYmZhOWQ2ZmU1ZmYzOTM0OGMxMDA5MTE0YTYzMTViMWUyMjU2NDU5MTM1MzQ4In0sImV4cCI6MTYzODg0MTIyN30.ZUiW_m3Li5eklc1cK5z2VOLVqlv9yPQ9ojHddegSiNKj5eEf8PoTsbzIKhHFkUkRtgArMTiJhmDRT_9L7vCKIg"

export URL="http://127.0.0.1:8003/query/QmdehxWNdDBNM5jXmb4qbCkGBpTW6ooVy17NYGoMEeowxt"


curl -i -X POST $URL \
-H 'Content-Type: application/json' \
-H "Authorization: Bearer $TOKEN" \
-d "{
  \"query\": { 
    \"query\": \"query { _metadata { indexerHealthy chain} }\" 
  }
}"
```

**Response**:

```json
{
  "data":{
    "_metadata": {
      "chain":"Darwinia",
      "indexerHealthy":false
    }
  }
}
```

#### Query with `operation_name` and `variables`

```sh
TIME_COST="\n\n%{time_connect} + %{time_starttransfer} = %{time_total}\n"

curl -w $TIME_COST -i -X POST $URL \
-H 'Content-Type: application/json' \
-H "Authorization: Bearer $TOKEN" \
-d "{
  \"query\": { 
    \"query\": \"query GetAccounts(\$first: Int\!) { accounts (first: \$first) { nodes { id } } }\",
    \"variables\": { \"first\": 5 },
    \"operationName\": \"GetAccounts\"
  }
}"
```

**Response**:

```json
{"data":{
  "accounts":{
    "nodes":[
      {"id":"2oacrSFsNu31PvuUDfULWE6oMHhSjtEk81moPCxX2SYXUuNE"},
      {"id":"2oafaTyZ9a9aoh8Cnhcr3e1LNrAiQdwi4kbeGmTCSTBARRHn"},
      {"id":"2oakar8GYiNytA4U68kKrfS2qpLfdGPEZjSCUVLYC8izRAGj"},
      {"id":"2oAserkFvEk5p4HMJaqRoDnedjaHzJLNPvyN5JaRLPhn4zpW"},
      {"id":"2oaY38m69Ditx8Rft5kdXPZgtzwuvpx42oFnLBeUyzfa2XfH"}
    ]}}}
```
