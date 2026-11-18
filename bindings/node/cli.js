#!/usr/bin/env node
// `npx drawland map …`: the command line, the same one `cargo install drawland-cli` installs.
process.exitCode = require('./native.js').cli(process.argv.slice(2))
