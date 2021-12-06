# Indexer Proxy

## Local Running

1. Start `coordinator service`

- `yarn start:db`
- `yarn start:testnet`

2. Start `proxy server`

- `cargo run`

## APIs

###  `/discovery/${id}`

```sh
curl -i -X GET http://127.0.0.1:8000/discovery/0x6c8212408c3c62fc78cbfa9d6fe5ff39348c1009114a6315b1e2256459135348
```

Response:

```json
{
  "uri": "/query/0x6c8212408c3c62fc78cbfa9d6fe5ff39348c1009114a6315b1e2256459135348"
}
```

### `/token?user_id=${user_id}&deployment_id=${id}`

```sh
curl -i -X GET http://127.0.0.1:8000/token?user_id="0x59ce189fd40611162017deb88d826C3485f41e0D"&deployment_id="0x6c8212408c3c62fc78cbfa9d6fe5ff39348c1009114a6315b1e2256459135348"
```

Response:

```json
{ 
  "token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzUxMiJ9.eyJ1c2VyIjp7InVzZXJfaWQiOiIweDU5Y2UxODlmZDQwNjExMTYyMDE3ZGViODhkODI2QzM0ODVmNDFlMEQiLCJkZXBsb3ltZW50X2lkIjoiMHg2YzgyMTI0MDhjM2M2MmZjNzhjYmZhOWQ2ZmU1ZmYzOTM0OGMxMDA5MTE0YTYzMTViMWUyMjU2NDU5MTM1MzQ4In0sImV4cCI6MTYzODg0MjA5MH0.4ej2RiEIPvSfKXisKCH2OYvu8WuLKMgKL59KlwpX6XTVUl0h57e63bdJjxxb109JwAGqkCVufKgj8m4OVETiyA"
}
```

### `/query/${id}`

```sh
TOKEN="eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzUxMiJ9.eyJ1c2VyIjp7InVzZXJfaWQiOiIweGVlcmZzZGZkc2YiLCJkZXBsb3ltZW50X2lkIjoiMHg2YzgyMTI0MDhjM2M2MmZjNzhjYmZhOWQ2ZmU1ZmYzOTM0OGMxMDA5MTE0YTYzMTViMWUyMjU2NDU5MTM1MzQ4In0sImV4cCI6MTYzODg0MTIyN30.ZUiW_m3Li5eklc1cK5z2VOLVqlv9yPQ9ojHddegSiNKj5eEf8PoTsbzIKhHFkUkRtgArMTiJhmDRT_9L7vCKIg"

ID="0x6c8212408c3c62fc78cbfa9d6fe5ff39348c1009114a6315b1e2256459135348"


curl -i -X POST "http://127.0.0.1:8000/query/$ID" -H 'Content-Type: application/json' -H "Authorization: Bearer $TOKEN" -d "{\"query\": { \"query\": \"{ _metadata { indexerHealthy chain} }\" } }"
```

Response:

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