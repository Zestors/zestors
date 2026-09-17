[private]
default:
    @just -l -u --list-submodules

supervise-example:
    @cargo run --example supervision

generate-openapi:
    @cargo run --package zestors-gen-oapi

generate-api-client: generate-openapi
    # docker run --rm \
    #     -v "$PWD:/local" \
    #     openapitools/openapi-generator-cli generate \
    #     -i /local/openapi.json \
    #     -g rust \
    #     -o /local/crates/client-api-gen \
    #     --library reqwest \
    #     --additional-properties=packageName=zestors-client-api-gen \
    #     --additional-properties=packageVersion=0.1.0
    # ploidy generate rust \
    #     openapi.json \
    #     -o crates/client-api-gen

    # oas3-gen generate -i openapi.json -o crates/client-api/src/oas3/types.rs
    oas3-gen generate client-mod -i openapi.json -o crates/client-api/src/oas3

mod inspector "crates/inspector"