pub mod cli;
pub mod index;
pub mod install;
pub mod plugins;
pub mod source;

mod fetch;
mod stage;
mod store;

#[cfg(test)]
mod install_tests;
#[cfg(test)]
mod manage_tests;
#[cfg(test)]
mod plugins_tests;
#[cfg(test)]
mod support;
