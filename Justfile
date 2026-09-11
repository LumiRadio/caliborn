entities:
    rm -rf src/entities
    sea generate entity -o src/entities --with-serde both --with-prelude none

fmt:
    cargo fmt --all
