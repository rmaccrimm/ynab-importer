docker pull openapitools/openapi-generator-cli
docker run --rm -v ${PWD}:/local openapitools/openapi-generator-cli generate `
    -i /local/ynab_spec.yaml `
    -g rust  `
    -o /local/api-lib `
    --additional-properties=packageName=ynab_api
