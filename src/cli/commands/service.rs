//! `service` subcommand handlers.

use crate::cli_repl;

pub(crate) fn cmd_repl() {
    {
        cli_repl::run_repl();
    }
}
