{
  description = "Open Labbook core module (Rust)";

  # Identical to a C++ module's inputs: the builder provides both the code
  # generator and the logos-rust-sdk source the crate links, so there is no
  # logos-rust-sdk input to keep in sync here.
  inputs = {
    logos-module-builder.url = "github:logos-co/logos-module-builder/0.3.0";
  };

  outputs = inputs@{ logos-module-builder, ... }:
    logos-module-builder.lib.mkLogosModule {
      src = ./.;
      configFile = ./metadata.json;
      flakeInputs = inputs;
    };
}
