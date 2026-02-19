# Plugin Authoring

Plugins are shared libraries that export `PneumaPluginVTable`.
The ABI is defined in `crates/pneuma-plugin/src/vtable.rs`.

Initial scaffold behavior only supports discovery/loading stubs;
full lifecycle callbacks will be implemented in later milestones.
