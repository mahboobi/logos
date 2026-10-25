{
  description = "Open Labbook UI module (C++ backend + QML view)";

  inputs = {
    logos-module-builder.url = "github:logos-co/logos-module-builder/0.3.0";
    # Sibling module flake. Single-line `path:` form is required — the flake.nix
    # parser scaffold uses is line-level and misses nested attrset forms
    # (docs/08 R8).
    olb_core.url = "path:../olb_core";
    # Without this follows, a second logos-module-builder node enters
    # flake.lock and silently wins under scaffold (docs/08 R8).
    olb_core.inputs.logos-module-builder.follows = "logos-module-builder";
  };

  outputs = inputs@{ logos-module-builder, ... }:
    logos-module-builder.lib.mkLogosQmlModule {
      src = ./.;
      configFile = ./metadata.json;
      flakeInputs = inputs;
    };
}
